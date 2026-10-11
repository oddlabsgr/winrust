//! Network thread for internet radio: HTTP body → ICY strip → byte pipe read by the decoder.

use crate::http;
use crate::icy::IcyReader;
use crate::types::EngineError;
use crossbeam_channel::{Receiver, RecvTimeoutError, bounded, unbounded};
use std::io::{self, Read};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

const CHUNK_BYTES: usize = 16 * 1024;
/// 64 chunks of 16 KiB: about a minute of 128 kbit/s audio buffered ahead at most.
const PIPE_CHUNKS: usize = 64;
const POLL: Duration = Duration::from_millis(50);

/// Cancellation token. The engine bumps a shared epoch counter whenever the user loads,
/// stops or quits; every token taken before the bump reports cancelled.
#[derive(Clone, Debug, Default)]
pub struct Cancel {
    epoch: Arc<AtomicU64>,
    seen: u64,
}

impl Cancel {
    pub fn new(epoch: Arc<AtomicU64>) -> Self {
        let seen = epoch.load(Ordering::Acquire);
        Cancel { epoch, seen }
    }

    pub fn is_cancelled(&self) -> bool {
        self.epoch.load(Ordering::Acquire) != self.seen
    }
}

type Chunk = Result<Vec<u8>, String>;

/// How much stream data waits to be read. The decode thread checks it before decoding, so a
/// stalled network never blocks it inside a read: it keeps handling commands and reporting
/// state and position meanwhile.
#[derive(Clone, Debug, Default)]
pub struct Backlog {
    bytes: Arc<AtomicUsize>,
    closed: Arc<AtomicBool>,
}

impl Backlog {
    /// True when `want` bytes can be read without waiting for the network, or when the
    /// stream has ended (reads then return what is left, then end).
    pub fn ready(&self, want: usize) -> bool {
        self.closed.load(Ordering::Acquire) || self.bytes.load(Ordering::Acquire) >= want
    }
}

/// Marks the stream ended when the network thread exits, however it exits.
struct Closer(Arc<AtomicBool>);

impl Drop for Closer {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

pub struct OpenStream {
    pub reader: PipeReader,
    pub content_type: Option<String>,
    pub bitrate: Option<u32>,
    pub titles: Receiver<String>,
    pub backlog: Backlog,
}

/// Connects on a background thread and waits for the response headers.
/// Returns early with an error if `cancel` fires first.
pub fn open(url: &str, cancel: Cancel) -> Result<OpenStream, EngineError> {
    let (meta_tx, meta_rx) = bounded::<Result<(Option<String>, Option<u32>), EngineError>>(1);
    let (chunk_tx, chunk_rx) = bounded::<Chunk>(PIPE_CHUNKS);
    let (title_tx, title_rx) = unbounded();
    let url_owned = url.to_string();
    let net_cancel = cancel.clone();
    let backlog = Backlog::default();
    let net_backlog = backlog.clone();
    thread::Builder::new()
        .name("winrust-net".into())
        .spawn(move || {
            let _closer = Closer(net_backlog.closed.clone());
            let resp = match http::open(&url_owned) {
                Ok(r) => r,
                Err(e) => {
                    let _ = meta_tx.send(Err(e));
                    return;
                }
            };
            let _ = meta_tx.send(Ok((resp.content_type.clone(), resp.bitrate)));
            let mut body: Box<dyn Read> = if resp.metaint > 0 {
                Box::new(IcyReader::new(resp.body, resp.metaint, title_tx))
            } else {
                resp.body
            };
            loop {
                if net_cancel.is_cancelled() {
                    return;
                }
                let mut buf = vec![0u8; CHUNK_BYTES];
                match body.read(&mut buf) {
                    Ok(0) => return,
                    Ok(n) => {
                        buf.truncate(n);
                        net_backlog.bytes.fetch_add(n, Ordering::AcqRel);
                        if chunk_tx.send(Ok(buf)).is_err() {
                            return;
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        let _ = chunk_tx.send(Err(e.to_string()));
                        return;
                    }
                }
            }
        })
        .map_err(|e| EngineError::Stream(format!("cannot start network thread: {e}")))?;

    let (content_type, bitrate) = loop {
        if cancel.is_cancelled() {
            return Err(EngineError::Stream("cancelled".into()));
        }
        match meta_rx.recv_timeout(POLL) {
            Ok(result) => break result?,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                return Err(EngineError::Stream("connection closed".into()));
            }
        }
    };
    Ok(OpenStream {
        reader: PipeReader {
            rx: chunk_rx,
            buf: Vec::new(),
            pos: 0,
            cancel,
            backlog: backlog.clone(),
        },
        content_type,
        bitrate,
        titles: title_rx,
        backlog,
    })
}

/// Blocking reader over the chunk pipe. Ends (EOF) when the stream closes or is cancelled.
pub struct PipeReader {
    rx: Receiver<Chunk>,
    buf: Vec<u8>,
    pos: usize,
    cancel: Cancel,
    backlog: Backlog,
}

impl Read for PipeReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.pos == self.buf.len() {
            if self.cancel.is_cancelled() {
                return Ok(0);
            }
            match self.rx.recv_timeout(POLL) {
                Ok(Ok(chunk)) => {
                    self.buf = chunk;
                    self.pos = 0;
                }
                Ok(Err(e)) => return Err(io::Error::other(e)),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
        let n = out.len().min(self.buf.len() - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        self.backlog.bytes.fetch_sub(n, Ordering::AcqRel);
        Ok(n)
    }
}

/// File extension symphonia should try first for a stream's Content-Type.
pub fn extension_for_mime(mime: &str) -> Option<&'static str> {
    match mime {
        "audio/mpeg" | "audio/mp3" | "audio/x-mpeg" => Some("mp3"),
        "audio/aac" | "audio/aacp" | "audio/x-aac" | "audio/x-aacp" => Some("aac"),
        "audio/ogg" | "application/ogg" | "audio/x-ogg" | "audio/vorbis" => Some("ogg"),
        "audio/flac" | "audio/x-flac" => Some("flac"),
        "audio/wav" | "audio/x-wav" | "audio/wave" => Some("wav"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    /// Serves one response then closes. Returns the URL.
    fn serve_once(response: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut req = [0u8; 1024];
            let _ = sock.read(&mut req);
            sock.write_all(&response).unwrap();
        });
        format!("http://{addr}/stream")
    }

    #[test]
    fn reads_icy_stream_and_titles() {
        let mut resp =
            b"ICY 200 OK\r\ncontent-type: audio/mpeg\r\nicy-metaint: 4\r\nicy-br: 128\r\n\r\n"
                .to_vec();
        resp.extend(b"AAAA");
        let meta = b"StreamTitle='Hello';\0\0\0\0\0\0\0\0\0\0\0\0";
        resp.push((meta.len() / 16) as u8);
        resp.extend(meta);
        resp.extend(b"BB");
        let url = serve_once(resp);
        let mut s = open(&url, Cancel::default()).unwrap();
        assert_eq!(s.content_type.as_deref(), Some("audio/mpeg"));
        assert_eq!(s.bitrate, Some(128));
        let mut body = Vec::new();
        s.reader.read_to_end(&mut body).unwrap();
        assert_eq!(body, b"AAAABB");
        assert_eq!(s.titles.try_recv().unwrap(), "Hello");
    }

    #[test]
    fn follows_redirect() {
        let target = serve_once(b"HTTP/1.0 200 OK\r\ncontent-type: audio/ogg\r\n\r\nOGG".to_vec());
        let first =
            serve_once(format!("HTTP/1.1 302 Found\r\nLocation: {target}\r\n\r\n").into_bytes());
        let mut s = open(&first, Cancel::default()).unwrap();
        let mut body = Vec::new();
        s.reader.read_to_end(&mut body).unwrap();
        assert_eq!(body, b"OGG");
    }

    #[test]
    fn http_error_is_reported() {
        let url = serve_once(b"HTTP/1.0 404 Not Found\r\n\r\n".to_vec());
        let e = open(&url, Cancel::default()).err().unwrap();
        assert_eq!(e, EngineError::Stream("server returned 404".into()));
    }

    #[test]
    fn connection_refused_is_reported() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let e = open(&format!("http://127.0.0.1:{port}/"), Cancel::default())
            .err()
            .unwrap();
        assert!(matches!(e, EngineError::Stream(_)), "{e:?}");
    }

    #[test]
    fn cancel_ends_reader() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut req = [0u8; 1024];
            let _ = sock.read(&mut req);
            sock.write_all(b"HTTP/1.0 200 OK\r\n\r\nX").unwrap();
            thread::sleep(Duration::from_secs(5));
        });
        let epoch = Arc::new(AtomicU64::new(0));
        let mut s = open(&format!("http://{addr}/"), Cancel::new(epoch.clone())).unwrap();
        let mut one = [0u8; 1];
        assert_eq!(s.reader.read(&mut one).unwrap(), 1);
        epoch.fetch_add(1, Ordering::AcqRel);
        let started = std::time::Instant::now();
        assert_eq!(s.reader.read(&mut one).unwrap(), 0);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn backlog_counts_unread_bytes_until_the_stream_ends() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (close_tx, close_rx) = bounded::<()>(1);
        thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut req = [0u8; 1024];
            let _ = sock.read(&mut req);
            sock.write_all(b"HTTP/1.0 200 OK\r\n\r\nABCDEFGH").unwrap();
            let _ = close_rx.recv();
        });
        let mut s = open(&format!("http://{addr}/"), Cancel::default()).unwrap();
        let wait = |ready: &dyn Fn() -> bool| {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !ready() {
                assert!(std::time::Instant::now() < deadline, "timed out");
                thread::sleep(Duration::from_millis(10));
            }
        };
        wait(&|| s.backlog.ready(8));
        assert!(!s.backlog.ready(9), "only 8 bytes arrived");
        let mut four = [0u8; 4];
        s.reader.read_exact(&mut four).unwrap();
        assert!(s.backlog.ready(4) && !s.backlog.ready(5), "4 bytes left");
        close_tx.send(()).unwrap();
        wait(&|| s.backlog.ready(1000));
    }

    #[test]
    fn mime_mapping() {
        assert_eq!(extension_for_mime("audio/aacp"), Some("aac"));
        assert_eq!(extension_for_mime("text/html"), None);
    }
}
