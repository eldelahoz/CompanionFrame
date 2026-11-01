//! Polling tail of one file: seek to the last offset, buffer a trailing partial line, and treat a
//! shorter or re-created file (game restart) as a fresh log.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::SystemTime;

const CHUNK: usize = 256 * 1024;
const MAX_CHUNKS_PER_POLL: usize = 8;
/// A file that never writes a newline must not grow the buffer forever.
const MAX_PARTIAL_LINE: usize = 1 << 20;

#[derive(Debug, PartialEq)]
pub enum Out {
    Line(String),
    /// The log was truncated, replaced or deleted; offsets and per-session parser state are stale.
    Reset,
}

pub struct Tail {
    path: PathBuf,
    file: Option<File>,
    offset: u64,
    created: Option<SystemTime>,
    rest: Vec<u8>,
    seen: bool,
    skip_existing: bool,
    pub bytes_read: u64,
}

impl Tail {
    /// With `skip_existing`, what is already in the file at the first poll is not replayed.
    pub fn new(path: PathBuf, skip_existing: bool) -> Self {
        Self {
            path,
            file: None,
            offset: 0,
            created: None,
            rest: Vec::new(),
            seen: false,
            skip_existing,
            bytes_read: 0,
        }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Reads whatever was appended since the last call. Returns whether the file exists.
    pub fn poll(&mut self, mut emit: impl FnMut(Out)) -> bool {
        let Ok(meta) = fs::metadata(&self.path) else {
            if self.seen {
                self.restart(&mut emit);
                self.seen = false;
                self.skip_existing = false;
            }
            return false;
        };
        let (len, created) = (meta.len(), meta.created().ok());
        if !self.seen {
            self.seen = true;
            self.created = created;
            if self.skip_existing {
                self.offset = len;
            }
        } else if len < self.offset || (created.is_some() && created != self.created) {
            self.restart(&mut emit);
            self.created = created;
        }
        if len > self.offset
            && let Err(e) = self.read_new(&mut emit)
        {
            tracing::warn!(path = %self.path.display(), error = %e, "ee.log read failed");
            self.file = None;
        }
        true
    }

    fn restart(&mut self, emit: &mut impl FnMut(Out)) {
        self.file = None;
        self.offset = 0;
        self.rest.clear();
        emit(Out::Reset);
    }

    fn read_new(&mut self, emit: &mut impl FnMut(Out)) -> std::io::Result<()> {
        if self.file.is_none() {
            self.file = Some(File::open(&self.path)?);
        }
        let file = self.file.as_mut().unwrap();
        file.seek(SeekFrom::Start(self.offset))?;
        let mut buf = vec![0u8; CHUNK];
        for _ in 0..MAX_CHUNKS_PER_POLL {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            self.offset += n as u64;
            self.bytes_read += n as u64;
            self.rest.extend_from_slice(&buf[..n]);
            let mut start = 0;
            while let Some(i) = self.rest[start..].iter().position(|&b| b == b'\n') {
                send_line(&self.rest[start..=start + i], emit);
                start += i + 1;
            }
            // One shift per chunk, not one per line.
            self.rest.drain(..start);
            if self.rest.len() > MAX_PARTIAL_LINE {
                send_line(&std::mem::take(&mut self.rest), emit);
            }
            if n < CHUNK {
                break;
            }
        }
        Ok(())
    }
}

fn send_line(bytes: &[u8], emit: &mut impl FnMut(Out)) {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_end_matches(['\r', '\n']);
    if !text.is_empty() {
        emit(Out::Line(text.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::eelog::tests::tmp_dir;
    use std::io::Write;

    fn poll(t: &mut Tail) -> Vec<Out> {
        let mut out = Vec::new();
        t.poll(|o| out.push(o));
        out
    }

    fn line(s: &str) -> Out {
        Out::Line(s.into())
    }

    fn append(path: &std::path::Path, data: &[u8]) {
        let mut f = File::options()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        f.write_all(data).unwrap();
    }

    #[test]
    fn skips_existing_content_then_follows_appends() {
        let p = tmp_dir("tail").join("EE.log");
        append(&p, b"old\n");
        let mut t = Tail::new(p.clone(), true);
        assert!(poll(&mut t).is_empty());
        append(&p, b"new\r\n\nnext\n");
        assert_eq!(poll(&mut t), [line("new"), line("next")]);
    }

    #[test]
    fn buffers_a_partial_line_and_a_split_utf8_char() {
        let p = tmp_dir("tail").join("EE.log");
        let mut t = Tail::new(p.clone(), false);
        let bytes = "Sys [Info]: caf\u{e9}\n".as_bytes();
        let cut = bytes.len() - 2; // inside the two-byte e-acute
        append(&p, &bytes[..cut]);
        assert!(poll(&mut t).is_empty());
        append(&p, &bytes[cut..]);
        assert_eq!(poll(&mut t), [line("Sys [Info]: caf\u{e9}")]);
    }

    #[test]
    fn truncation_resets_and_rereads_from_the_start() {
        let p = tmp_dir("tail").join("EE.log");
        append(&p, b"one\ntwo\npartial");
        let mut t = Tail::new(p.clone(), false);
        assert_eq!(poll(&mut t), [line("one"), line("two")]);
        fs::write(&p, "fresh\n").unwrap();
        // The dropped "partial" must not be glued onto the new session's first line.
        assert_eq!(poll(&mut t), [Out::Reset, line("fresh")]);
        assert_eq!(t.bytes_read, 15 + 6);
    }

    #[test]
    fn missing_file_is_read_from_the_start_when_it_appears() {
        let p = tmp_dir("tail").join("EE.log");
        let mut t = Tail::new(p.clone(), false);
        assert!(!t.poll(|_| {}));
        append(&p, b"boot\n");
        assert_eq!(poll(&mut t), [line("boot")]);
    }

    #[test]
    fn deleted_file_resets_once_and_goes_inactive() {
        let p = tmp_dir("tail").join("EE.log");
        append(&p, b"a\n");
        let mut t = Tail::new(p.clone(), false);
        poll(&mut t);
        fs::remove_file(&p).unwrap();
        let mut out = Vec::new();
        assert!(!t.poll(|o| out.push(o)));
        assert_eq!(out, [Out::Reset]);
        assert!(poll(&mut t).is_empty());
        append(&p, b"b\n");
        assert_eq!(poll(&mut t), [line("b")]);
    }

    #[test]
    fn unbroken_garbage_is_flushed_not_hoarded() {
        let p = tmp_dir("tail").join("EE.log");
        append(&p, &vec![b'x'; MAX_PARTIAL_LINE + 1]);
        let mut t = Tail::new(p, false);
        assert_eq!(poll(&mut t).len(), 1);
    }
}
