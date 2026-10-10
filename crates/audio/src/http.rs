//! Minimal HTTP/1.0 client for audio streams.
//!
//! Hand-rolled because old SHOUTcast servers answer `ICY 200 OK`, which general-purpose
//! HTTP clients reject. Supports http/https, redirects, and the ICY response headers.

use crate::types::EngineError;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;
use url::Url;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_REDIRECTS: usize = 5;
const MAX_HEAD_BYTES: usize = 64 * 1024;

pub struct StreamResponse {
    pub body: Box<dyn Read + Send>,
    pub content_type: Option<String>,
    /// Bytes of audio between ICY metadata blocks; 0 when the server sends none.
    pub metaint: usize,
    pub bitrate: Option<u32>,
    pub name: Option<String>,
}

trait Conn: Read + Write + Send {}
impl<T: Read + Write + Send> Conn for T {}

/// Connects and returns the response body positioned at the first audio byte.
pub fn open(url: &str) -> Result<StreamResponse, EngineError> {
    let mut url = Url::parse(url).map_err(|e| err(format!("bad URL {url}: {e}")))?;
    for _ in 0..=MAX_REDIRECTS {
        let mut reader = BufReader::new(connect(&url)?);
        let head = read_head(&mut reader)?;
        match head.status {
            200 => {
                return Ok(StreamResponse {
                    body: Box::new(reader),
                    content_type: head.header("content-type").map(|v| {
                        v.split(';')
                            .next()
                            .unwrap_or("")
                            .trim()
                            .to_ascii_lowercase()
                    }),
                    metaint: head
                        .header("icy-metaint")
                        .and_then(|v| v.trim().parse().ok())
                        .unwrap_or(0),
                    bitrate: head
                        .header("icy-br")
                        .and_then(|v| v.split(',').next()?.trim().parse().ok()),
                    name: head.header("icy-name").map(|v| v.trim().to_string()),
                });
            }
            301 | 302 | 303 | 307 | 308 => {
                let location = head
                    .header("location")
                    .ok_or_else(|| err("redirect without Location".into()))?;
                url = url
                    .join(location.trim())
                    .map_err(|e| err(format!("bad redirect: {e}")))?;
            }
            code => return Err(err(format!("server returned {code}"))),
        }
    }
    Err(err("too many redirects".into()))
}

fn err(msg: String) -> EngineError {
    EngineError::Stream(msg)
}

fn connect(url: &Url) -> Result<Box<dyn Conn>, EngineError> {
    let host = url
        .host_str()
        .ok_or_else(|| err(format!("no host in {url}")))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| err(format!("no port for {url}")))?;
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|e| err(format!("cannot resolve {host}: {e}")))?;
    let mut last = err(format!("no address for {host}"));
    let mut tcp = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            Ok(s) => {
                tcp = Some(s);
                break;
            }
            Err(e) => last = err(format!("cannot connect to {host}: {e}")),
        }
    }
    let tcp = tcp.ok_or(last)?;
    tcp.set_read_timeout(Some(READ_TIMEOUT))
        .map_err(|e| err(e.to_string()))?;
    let _ = tcp.set_nodelay(true);

    let mut conn: Box<dyn Conn> = match url.scheme() {
        "http" | "icy" => Box::new(tcp),
        "https" => Box::new(tls(host, tcp)?),
        other => return Err(err(format!("unsupported scheme {other}"))),
    };
    let path = &url[url::Position::BeforePath..url::Position::AfterQuery];
    let host_header = match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host.to_string(),
    };
    let request = format!(
        "GET {} HTTP/1.0\r\nHost: {host_header}\r\nUser-Agent: Winrust/{}\r\nAccept: */*\r\nIcy-MetaData: 1\r\nConnection: close\r\n\r\n",
        if path.is_empty() { "/" } else { path },
        env!("CARGO_PKG_VERSION"),
    );
    conn.write_all(request.as_bytes())
        .map_err(|e| err(format!("send failed: {e}")))?;
    conn.flush().map_err(|e| err(format!("send failed: {e}")))?;
    Ok(conn)
}

fn tls(
    host: &str,
    tcp: TcpStream,
) -> Result<rustls::StreamOwned<rustls::ClientConnection, TcpStream>, EngineError> {
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| err(format!("TLS setup: {e}")))?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|e| err(format!("bad TLS name {host}: {e}")))?;
    let conn = rustls::ClientConnection::new(Arc::new(config), name)
        .map_err(|e| err(format!("TLS: {e}")))?;
    Ok(rustls::StreamOwned::new(conn, tcp))
}

struct Head {
    status: u16,
    headers: Vec<(String, String)>,
}

impl Head {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn read_head(reader: &mut impl BufRead) -> Result<Head, EngineError> {
    let mut total = 0;
    let mut line = String::new();
    let mut next_line = |line: &mut String| -> Result<(), EngineError> {
        line.clear();
        let mut raw = Vec::new();
        let n = reader
            .read_until(b'\n', &mut raw)
            .map_err(|e| err(format!("read failed: {e}")))?;
        total += n;
        if n == 0 || total > MAX_HEAD_BYTES {
            return Err(err("malformed response header".into()));
        }
        line.push_str(String::from_utf8_lossy(&raw).trim_end_matches(['\r', '\n']));
        Ok(())
    };
    next_line(&mut line)?;
    let status = parse_status(&line).ok_or_else(|| err(format!("bad status line: {line}")))?;
    let mut headers = Vec::new();
    loop {
        next_line(&mut line)?;
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    Ok(Head { status, headers })
}

/// Accepts `HTTP/1.x 200 OK` and SHOUTcast's `ICY 200 OK`.
fn parse_status(line: &str) -> Option<u16> {
    let mut parts = line.split_whitespace();
    let proto = parts.next()?;
    if !(proto.starts_with("HTTP/") || proto == "ICY") {
        return None;
    }
    parts.next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_lines() {
        assert_eq!(parse_status("HTTP/1.1 200 OK"), Some(200));
        assert_eq!(parse_status("HTTP/1.0 302 Found"), Some(302));
        assert_eq!(parse_status("ICY 200 OK"), Some(200));
        assert_eq!(parse_status("SSH-2.0-OpenSSH"), None);
        assert_eq!(parse_status(""), None);
    }

    #[test]
    fn reads_head_and_leaves_body() {
        let raw = b"ICY 200 OK\r\nicy-metaint: 8192\r\nContent-Type: audio/mpeg\r\nicy-br:128\r\n\r\nBODY";
        let mut reader = BufReader::new(&raw[..]);
        let head = read_head(&mut reader).unwrap();
        assert_eq!(head.status, 200);
        assert_eq!(head.header("ICY-METAINT"), Some("8192"));
        assert_eq!(head.header("content-type"), Some("audio/mpeg"));
        let mut body = String::new();
        reader.read_to_string(&mut body).unwrap();
        assert_eq!(body, "BODY");
    }

    #[test]
    fn truncated_head_is_error() {
        let mut reader = BufReader::new(&b"HTTP/1.0 200 OK\r\nfoo: bar\r\n"[..]);
        assert!(read_head(&mut reader).is_err());
    }
}
