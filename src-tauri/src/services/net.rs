//! HTTP for game data: timeout, User-Agent, body size cap, ETag revalidation.

use std::time::Duration;

use reqwest::{Client, StatusCode, header};

pub fn client() -> reqwest::Result<Client> {
    Client::builder()
        .user_agent(concat!("CompanionFrame/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()
}

/// `client()` that reports a redirect instead of following it.
pub fn client_no_redirect() -> reqwest::Result<Client> {
    Client::builder()
        .user_agent(concat!("CompanionFrame/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
}

/// For downloads and authenticated calls: HTTPS only (redirects included), few redirects, and an
/// idle timeout instead of a total one so a slow large body is not cut off.
/// `follow_redirects: false` for calls whose URL carries a secret: a redirect is an error.
pub fn https_client(follow_redirects: bool) -> reqwest::Result<Client> {
    Client::builder()
        .user_agent(concat!("CompanionFrame/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .redirect(if follow_redirects {
            reqwest::redirect::Policy::limited(4)
        } else {
            reqwest::redirect::Policy::none()
        })
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .build()
}

#[derive(Debug, PartialEq)]
pub enum Fetched {
    Modified { body: Vec<u8>, etag: Option<String> },
    NotModified,
}

/// GET `url`, sending `If-None-Match` when `etag` is known. Errors on non-2xx/304 or a body over `max_bytes`.
pub async fn fetch(
    client: &Client,
    url: &str,
    etag: Option<&str>,
    max_bytes: usize,
) -> Result<Fetched, String> {
    let mut req = client.get(url);
    if let Some(e) = etag {
        req = req.header(header::IF_NONE_MATCH, e);
    }
    let mut res = req.send().await.map_err(|e| e.to_string())?;
    if res.status() == StatusCode::NOT_MODIFIED {
        return Ok(Fetched::NotModified);
    }
    if !res.status().is_success() {
        return Err(format!("{url}: HTTP {}", res.status()));
    }
    let etag = res
        .headers()
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let mut body = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(|e| e.to_string())? {
        if body.len() + chunk.len() > max_bytes {
            return Err(format!("{url}: body exceeds {max_bytes} bytes"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Fetched::Modified { body, etag })
}

#[cfg(test)]
pub mod test_server {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// One-route HTTP server: 200 with ETag "v1" (or 304 when `If-None-Match: "v1"`), `body` as payload.
    /// Returns the URL and a request counter.
    pub fn spawn(body: &'static str) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/data", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                let mut buf = [0u8; 2048];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_lowercase();
                counter.fetch_add(1, Ordering::SeqCst);
                let res = if req.contains("if-none-match: \"v1\"") {
                    "HTTP/1.1 304 Not Modified\r\nConnection: close\r\n\r\n".to_string()
                } else {
                    format!(
                        "HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                };
                let _ = s.write_all(res.as_bytes());
            }
        });
        (url, hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[tokio::test]
    async fn etag_roundtrip_gives_304() {
        let (url, hits) = test_server::spawn("hello");
        let c = client().unwrap();
        let first = fetch(&c, &url, None, 1024).await.unwrap();
        assert_eq!(
            first,
            Fetched::Modified {
                body: b"hello".to_vec(),
                etag: Some("\"v1\"".into())
            }
        );
        assert_eq!(
            fetch(&c, &url, Some("\"v1\""), 1024).await.unwrap(),
            Fetched::NotModified
        );
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn rejects_oversized_body() {
        let (url, _) = test_server::spawn("hello world");
        let err = fetch(&client().unwrap(), &url, None, 4).await.unwrap_err();
        assert!(err.contains("exceeds"), "{err}");
    }

    #[tokio::test]
    async fn rejects_error_status() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", l.local_addr().unwrap());
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut s, _) = l.accept().unwrap();
            let _ = s.read(&mut [0u8; 512]);
            let _ =
                s.write_all(b"HTTP/1.1 500 Oops\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        });
        let err = fetch(&client().unwrap(), &url, None, 16).await.unwrap_err();
        assert!(err.contains("500"), "{err}");
    }
}
