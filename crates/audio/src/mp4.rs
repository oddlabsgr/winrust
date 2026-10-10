//! Encoder delay and padding ("gapless" info) for MP4/M4A files.
//!
//! symphonia 0.6 parses MP4 edit lists but does not apply them, and ignores iTunes'
//! `iTunSMPB` tag, so AAC files would play encoder priming at the start and padding at
//! the end. This module reads both so `Decoder` can trim them.

use std::io::{Read, Seek, SeekFrom};

/// Largest `moov` box we load. It holds only metadata; the audio lives in `mdat`.
const MAX_MOOV: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gapless {
    /// Leading decoded frames to drop (encoder priming).
    pub delay: u64,
    /// Frames of real audio after the delay, when known.
    pub valid: Option<u64>,
}

/// Reads gapless info from an MP4/M4A file. `None` if it is not MP4 or carries none.
pub fn read_gapless<R: Read + Seek>(file: &mut R, sample_rate: u32) -> Option<Gapless> {
    file.seek(SeekFrom::Start(0)).ok()?;
    let mut first = true;
    loop {
        let mut head = [0u8; 8];
        file.read_exact(&mut head).ok()?;
        let kind = &head[4..8];
        if first && kind != b"ftyp" {
            return None;
        }
        first = false;
        let (header, size) = match be_u32(&head, 0)? {
            1 => {
                let mut big = [0u8; 8];
                file.read_exact(&mut big).ok()?;
                (16, u64::from_be_bytes(big))
            }
            0 if kind == b"moov" => {
                let mut buf = Vec::new();
                file.take(MAX_MOOV).read_to_end(&mut buf).ok()?;
                return parse_moov(&buf, sample_rate);
            }
            0 => return None,
            n => (8, u64::from(n)),
        };
        let payload = size.checked_sub(header)?;
        if kind == b"moov" {
            if payload > MAX_MOOV {
                return None;
            }
            let mut buf = vec![0u8; usize::try_from(payload).ok()?];
            file.read_exact(&mut buf).ok()?;
            return parse_moov(&buf, sample_rate);
        }
        file.seek(SeekFrom::Current(i64::try_from(payload).ok()?))
            .ok()?;
    }
}

/// Gapless info from the payload of a `moov` box. `iTunSMPB` wins over the edit list.
pub fn parse_moov(moov: &[u8], sample_rate: u32) -> Option<Gapless> {
    if let Some(g) = itunsmpb(moov) {
        return Some(g);
    }
    let movie_timescale = timescale(child(moov, b"mvhd")?)?;
    let trak = boxes(moov)
        .filter(|(kind, _)| kind == b"trak")
        .map(|(_, payload)| payload)
        .find(|trak| is_audio(trak))?;
    let media_timescale = timescale(child(child(trak, b"mdia")?, b"mdhd")?)?;
    let (segment, media_time) = first_edit(child(child(trak, b"edts")?, b"elst")?)?;
    let rate = u64::from(sample_rate);
    let delay = media_time.checked_mul(rate)? / u64::from(media_timescale);
    let valid = match segment {
        0 => None,
        s => Some(s.checked_mul(rate)? / u64::from(movie_timescale)),
    };
    (delay > 0 || valid.is_some()).then_some(Gapless { delay, valid })
}

/// Iterates the boxes in `data` as (type, payload). Stops at the first malformed box.
fn boxes(data: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    let mut rest = data;
    std::iter::from_fn(move || {
        let size32 = be_u32(rest, 0)?;
        let kind = rest.get(4..8)?;
        let (header, size) = match size32 {
            0 => (8, rest.len()),
            1 => (16, usize::try_from(be_u64(rest, 8)?).ok()?),
            n => (8, usize::try_from(n).ok()?),
        };
        if size < header || size > rest.len() {
            rest = &[];
            return None;
        }
        let payload = &rest[header..size];
        rest = &rest[size..];
        Some((kind, payload))
    })
}

fn child<'a>(data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(data)
        .find(|(k, _)| k == kind)
        .map(|(_, payload)| payload)
}

fn be_u32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

fn be_u64(data: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(data.get(at..at + 8)?.try_into().ok()?))
}

/// Timescale of an `mvhd`/`mdhd` full box (version 1 has 64-bit creation/modification times).
fn timescale(full_box: &[u8]) -> Option<u32> {
    let at = if *full_box.first()? == 1 {
        4 + 16
    } else {
        4 + 8
    };
    be_u32(full_box, at).filter(|t| *t > 0)
}

fn is_audio(trak: &[u8]) -> bool {
    let handler = child(trak, b"mdia").and_then(|mdia| child(mdia, b"hdlr"));
    handler.and_then(|h| h.get(8..12)) == Some(b"soun".as_slice())
}

/// First non-empty edit as (segment duration in movie timescale, media time in media timescale).
fn first_edit(elst: &[u8]) -> Option<(u64, u64)> {
    let version = *elst.first()?;
    let count = be_u32(elst, 4)? as usize;
    let entry = if version == 1 { 20 } else { 12 };
    (0..count).find_map(|i| {
        let at = 8 + i * entry;
        let (segment, media_time) = if version == 1 {
            (be_u64(elst, at)?, be_u64(elst, at + 8)? as i64)
        } else {
            (
                u64::from(be_u32(elst, at)?),
                i64::from(be_u32(elst, at + 4)? as i32),
            )
        };
        // media_time -1 marks an empty edit (a leading gap), not a trim.
        (media_time >= 0).then_some((segment, media_time as u64))
    })
}

/// iTunes' ` 00000000 <delay> <padding> <original frames> ...` tag (hex fields).
fn itunsmpb(moov: &[u8]) -> Option<Gapless> {
    let meta = child(child(moov, b"udta")?, b"meta")?;
    // ISO `meta` is a full box (4 bytes of version/flags); QuickTime's is not.
    let meta = if meta.get(..4) == Some(&[0, 0, 0, 0]) {
        &meta[4..]
    } else {
        meta
    };
    let ilst = child(meta, b"ilst")?;
    let value = boxes(ilst)
        .filter(|(kind, _)| kind == b"----")
        .find_map(|(_, item)| {
            let name = child(item, b"name")?;
            (name.get(4..) == Some(b"iTunSMPB".as_slice())).then(|| child(item, b"data"))?
        })?;
    let text = std::str::from_utf8(value.get(8..)?).ok()?;
    let fields: Vec<u64> = text
        .split_whitespace()
        .map(|f| u64::from_str_radix(f, 16))
        .collect::<Result<_, _>>()
        .ok()?;
    let delay = *fields.get(1)?;
    let valid = fields.get(3).copied().filter(|n| *n > 0);
    Some(Gapless { delay, valid })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn bx(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }

    /// Full box: version + 24-bit flags, then payload.
    fn full(kind: &[u8; 4], version: u8, payload: &[u8]) -> Vec<u8> {
        let mut p = vec![version, 0, 0, 0];
        p.extend_from_slice(payload);
        bx(kind, &p)
    }

    fn mvhd(timescale: u32) -> Vec<u8> {
        let mut p = vec![0u8; 8]; // creation, modification
        p.extend(timescale.to_be_bytes());
        p.extend(0u32.to_be_bytes()); // duration
        full(b"mvhd", 0, &p)
    }

    fn mdia(timescale: u32, handler: &[u8; 4]) -> Vec<u8> {
        let mut m = vec![0u8; 8];
        m.extend(timescale.to_be_bytes());
        m.extend(0u32.to_be_bytes());
        let mut h = vec![0u8; 4]; // pre_defined
        h.extend_from_slice(handler);
        h.extend([0u8; 12]);
        let mut payload = full(b"mdhd", 0, &m);
        payload.extend(full(b"hdlr", 0, &h));
        bx(b"mdia", &payload)
    }

    /// Version-0 edit list from (segment_duration, media_time) pairs.
    fn edts(entries: &[(u32, i32)]) -> Vec<u8> {
        let mut p = (entries.len() as u32).to_be_bytes().to_vec();
        for &(dur, time) in entries {
            p.extend(dur.to_be_bytes());
            p.extend(time.to_be_bytes());
            p.extend(0x0001_0000u32.to_be_bytes()); // media rate 1.0
        }
        bx(b"edts", &full(b"elst", 0, &p))
    }

    fn trak(parts: &[Vec<u8>]) -> Vec<u8> {
        bx(b"trak", &parts.concat())
    }

    fn itunsmpb(value: &str) -> Vec<u8> {
        let mut mean = vec![0u8; 4];
        mean.extend_from_slice(b"com.apple.iTunes");
        let mut name = vec![0u8; 4];
        name.extend_from_slice(b"iTunSMPB");
        let mut data = vec![0, 0, 0, 1, 0, 0, 0, 0]; // type UTF-8, locale
        data.extend_from_slice(value.as_bytes());
        let dash = bx(
            b"----",
            &[bx(b"mean", &mean), bx(b"name", &name), bx(b"data", &data)].concat(),
        );
        let ilst = bx(b"ilst", &dash);
        let meta = full(b"meta", 0, &ilst);
        bx(b"udta", &meta)
    }

    fn moov_payload(parts: &[Vec<u8>]) -> Vec<u8> {
        parts.concat()
    }

    #[test]
    fn edit_list_gives_delay_and_length() {
        let moov = moov_payload(&[
            mvhd(1000),
            trak(&[mdia(44_100, b"soun"), edts(&[(1000, 1024)])]),
        ]);
        assert_eq!(
            parse_moov(&moov, 44_100),
            Some(Gapless {
                delay: 1024,
                valid: Some(44_100)
            })
        );
    }

    #[test]
    fn timescales_are_converted_to_frames() {
        // Movie timescale 600, media timescale 88 200 (twice the sample rate).
        let moov = moov_payload(&[
            mvhd(600),
            trak(&[mdia(88_200, b"soun"), edts(&[(600, 4224)])]),
        ]);
        assert_eq!(
            parse_moov(&moov, 44_100),
            Some(Gapless {
                delay: 2112,
                valid: Some(44_100)
            })
        );
    }

    #[test]
    fn empty_edit_is_skipped() {
        let moov = moov_payload(&[
            mvhd(1000),
            trak(&[mdia(44_100, b"soun"), edts(&[(500, -1), (1000, 1024)])]),
        ]);
        assert_eq!(
            parse_moov(&moov, 44_100),
            Some(Gapless {
                delay: 1024,
                valid: Some(44_100)
            })
        );
    }

    #[test]
    fn itunsmpb_takes_priority() {
        let moov = moov_payload(&[
            mvhd(1000),
            trak(&[mdia(44_100, b"soun"), edts(&[(1000, 1024)])]),
            itunsmpb(" 00000000 00000840 000001CA 0000000000037DF6 00000000 00000000"),
        ]);
        assert_eq!(
            parse_moov(&moov, 44_100),
            Some(Gapless {
                delay: 0x840,
                valid: Some(0x37DF6)
            })
        );
    }

    #[test]
    fn video_tracks_are_ignored() {
        let moov = moov_payload(&[
            mvhd(1000),
            trak(&[mdia(90_000, b"vide"), edts(&[(1000, 3000)])]),
            trak(&[mdia(44_100, b"soun")]),
        ]);
        assert_eq!(parse_moov(&moov, 44_100), None);
    }

    #[test]
    fn zero_delay_without_length_is_none() {
        let moov = moov_payload(&[mvhd(1000), trak(&[mdia(44_100, b"soun"), edts(&[(0, 0)])])]);
        assert_eq!(parse_moov(&moov, 44_100), None);
    }

    #[test]
    fn garbage_and_truncation_never_panic() {
        let moov = moov_payload(&[
            mvhd(1000),
            trak(&[mdia(44_100, b"soun"), edts(&[(1000, 1024)])]),
        ]);
        for cut in 0..moov.len() {
            let _ = parse_moov(&moov[..cut], 44_100);
        }
        assert_eq!(parse_moov(&[0xff; 64], 44_100), None);
    }

    #[test]
    fn reads_moov_after_media_data() {
        let moov = bx(
            b"moov",
            &moov_payload(&[
                mvhd(1000),
                trak(&[mdia(44_100, b"soun"), edts(&[(1000, 1024)])]),
            ]),
        );
        let mut file = bx(b"ftyp", b"M4A \0\0\0\0");
        file.extend(bx(b"mdat", &[0u8; 5000]));
        file.extend(moov);
        assert_eq!(
            read_gapless(&mut Cursor::new(file), 44_100),
            Some(Gapless {
                delay: 1024,
                valid: Some(44_100)
            })
        );
    }

    #[test]
    fn non_mp4_files_are_none() {
        assert_eq!(
            read_gapless(&mut Cursor::new(b"RIFF\0\0\0\0WAVEfmt ".to_vec()), 44_100),
            None
        );
        assert_eq!(read_gapless(&mut Cursor::new(Vec::new()), 44_100), None);
    }
}
