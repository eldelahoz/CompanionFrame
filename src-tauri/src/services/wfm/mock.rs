//! Scripted local HTTP server for tests: replies in order, records every raw request. No real network.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

pub struct Reply {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: String,
}

pub fn reply(status: u16, body: &str) -> Reply {
    Reply {
        status,
        headers: vec![],
        body: body.into(),
    }
}

/// Closes the connection without answering: what a network failure looks like to the client.
pub fn dropped() -> Reply {
    reply(0, "")
}

impl Reply {
    pub fn header(mut self, k: &'static str, v: &str) -> Self {
        self.headers.push((k, v.into()));
        self
    }
}

pub struct Mock {
    pub url: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl Mock {
    /// Lowercased raw requests (request line, headers, body) received so far.
    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

fn read_request(s: &mut std::net::TcpStream) -> String {
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = s.read(&mut buf).unwrap_or(0);
        data.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&data).to_lowercase();
        if let Some(end) = text.find("\r\n\r\n") {
            let len = text[..end]
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if data.len() >= end + 4 + len {
                return text;
            }
        }
        if n == 0 {
            return text;
        }
    }
}

pub fn spawn(script: Vec<Reply>) -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = requests.clone();
    std::thread::spawn(move || {
        let mut script = script.into_iter();
        for mut s in listener.incoming().flatten() {
            let req = read_request(&mut s);
            log.lock().unwrap().push(req);
            let r = script
                .next()
                .unwrap_or_else(|| reply(599, "script exhausted"));
            if r.status == 0 {
                continue;
            }
            let mut head = format!(
                "HTTP/1.1 {} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
                r.status,
                r.body.len()
            );
            for (k, v) in r.headers {
                head.push_str(&format!("{k}: {v}\r\n"));
            }
            let _ = s.write_all(format!("{head}\r\n{}", r.body).as_bytes());
        }
    });
    Mock { url, requests }
}
