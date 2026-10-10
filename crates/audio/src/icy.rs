//! SHOUTcast/Icecast in-band metadata ("ICY") handling.

use crossbeam_channel::Sender;
use std::io::{self, Read};

/// Wraps a stream body and removes the metadata block inserted every `metaint` bytes.
/// Each non-empty `StreamTitle` is sent to `titles`.
pub struct IcyReader<R> {
    inner: R,
    metaint: usize,
    until_meta: usize,
    titles: Sender<String>,
}

impl<R: Read> IcyReader<R> {
    pub fn new(inner: R, metaint: usize, titles: Sender<String>) -> Self {
        IcyReader {
            inner,
            metaint,
            until_meta: metaint,
            titles,
        }
    }

    fn read_metadata(&mut self) -> io::Result<()> {
        let mut len = [0u8; 1];
        self.inner.read_exact(&mut len)?;
        let size = usize::from(len[0]) * 16;
        if size > 0 {
            let mut block = vec![0u8; size];
            self.inner.read_exact(&mut block)?;
            if let Some(title) = parse_stream_title(&block) {
                let _ = self.titles.send(title);
            }
        }
        self.until_meta = self.metaint;
        Ok(())
    }
}

impl<R: Read> Read for IcyReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.metaint == 0 || buf.is_empty() {
            return self.inner.read(buf);
        }
        if self.until_meta == 0 {
            self.read_metadata()?;
        }
        let want = buf.len().min(self.until_meta);
        let n = self.inner.read(&mut buf[..want])?;
        self.until_meta -= n;
        Ok(n)
    }
}

/// Extracts the title from a metadata block like `StreamTitle='Artist - Song';StreamUrl='';`.
///
/// The title ends at the first `';` so apostrophes inside the title survive.
/// Bytes are decoded as UTF-8 when valid, otherwise as Latin-1 (common on old servers).
pub fn parse_stream_title(block: &[u8]) -> Option<String> {
    let text = decode_text(block);
    let start = text.find("StreamTitle='")? + "StreamTitle='".len();
    let rest = &text[start..];
    let end = rest
        .find("';")
        .or_else(|| rest.rfind('\''))
        .unwrap_or(rest.len());
    let title = rest[..end].trim();
    (!title.is_empty()).then(|| title.to_string())
}

fn decode_text(bytes: &[u8]) -> String {
    let bytes = match bytes.iter().position(|&b| b == 0) {
        Some(nul) => &bytes[..nul],
        None => bytes,
    };
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;

    fn block(meta: &str) -> Vec<u8> {
        let mut m = meta.as_bytes().to_vec();
        m.resize(m.len().div_ceil(16) * 16, 0);
        let mut out = vec![(m.len() / 16) as u8];
        out.extend(m);
        out
    }

    #[test]
    fn parses_plain_title() {
        assert_eq!(
            parse_stream_title(b"StreamTitle='Daft Punk - One More Time';StreamUrl='';\0\0"),
            Some("Daft Punk - One More Time".into())
        );
    }

    #[test]
    fn keeps_apostrophes_inside_title() {
        assert_eq!(
            parse_stream_title(b"StreamTitle='Guns N' Roses - Don't Cry';"),
            Some("Guns N' Roses - Don't Cry".into())
        );
    }

    #[test]
    fn empty_title_is_none() {
        assert_eq!(parse_stream_title(b"StreamTitle='';"), None);
        assert_eq!(parse_stream_title(b"garbage"), None);
    }

    #[test]
    fn latin1_fallback() {
        assert_eq!(
            parse_stream_title(b"StreamTitle='Caf\xe9';"),
            Some("Café".into())
        );
    }

    #[test]
    fn strips_metadata_and_reports_titles() {
        let mut data = b"abcd".to_vec();
        data.extend(block("StreamTitle='One';"));
        data.extend(b"efgh");
        data.push(0); // empty metadata block
        data.extend(b"ij");
        let (tx, rx) = unbounded();
        let mut r = IcyReader::new(&data[..], 4, tx);
        let mut audio = Vec::new();
        r.read_to_end(&mut audio).unwrap();
        assert_eq!(audio, b"abcdefghij");
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), vec!["One".to_string()]);
    }

    #[test]
    fn zero_metaint_passes_through() {
        let (tx, _rx) = unbounded();
        let mut r = IcyReader::new(&b"raw bytes"[..], 0, tx);
        let mut out = Vec::new();
        r.read_to_end(&mut out).unwrap();
        assert_eq!(out, b"raw bytes");
    }
}
