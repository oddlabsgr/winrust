//! symphonia wrapper: opens a source and yields interleaved stereo f32 at the source rate.

use crate::dsp::to_stereo;
use crate::types::{EngineError, Source, TrackInfo};
use std::fs::File;
use std::path::Path;
use std::time::Duration;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::{MetadataOptions, StandardTag, Tag};
use symphonia::core::units::{Time, TimeBase};

pub struct Decoder {
    source: Source,
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    sample_rate: u32,
    channels: usize,
    info: TrackInfo,
    scratch: Vec<f32>,
    /// Source frames still to drop after an accurate seek.
    skip_frames: u64,
    bitrate_bytes: u64,
    bitrate_frames: u64,
    pending_title: Option<String>,
}

impl Decoder {
    pub fn open_file(path: &Path) -> Result<Self, EngineError> {
        let source = Source::File(path.to_path_buf());
        let file = File::open(path).map_err(|e| EngineError::Open {
            path: source.to_string(),
            reason: e.to_string(),
        })?;
        let byte_len = file.metadata().map(|m| m.len()).ok();
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let mut dec = Self::open_media(source, Box::new(file), &hint)?;
        if let (Some(len), Some(d)) = (byte_len, dec.info.duration)
            && d.as_secs_f64() > 0.0
        {
            dec.info.kbps = Some((len as f64 * 8.0 / d.as_secs_f64() / 1000.0).round() as u32);
        }
        Ok(dec)
    }

    /// Opens any media source. `hint` carries a file extension or MIME type when known.
    pub fn open_media(
        source: Source,
        media: Box<dyn MediaSource>,
        hint: &Hint,
    ) -> Result<Self, EngineError> {
        let seekable = media.is_seekable();
        let mss = MediaSourceStream::new(media, Default::default());
        let format = symphonia::default::get_probe()
            .probe(
                hint,
                mss,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| EngineError::Unsupported(format!("{source}: {e}")))?;
        let sel = select_track(format.as_ref(), &source)?;
        let info = TrackInfo {
            source: source.clone(),
            kbps: None,
            sample_rate: sel.sample_rate,
            channels: sel.channels as u16,
            duration: sel.duration,
            seekable,
        };
        let mut dec = Decoder {
            source,
            format,
            decoder: sel.decoder,
            track_id: sel.track_id,
            time_base: sel.time_base,
            sample_rate: sel.sample_rate,
            channels: sel.channels,
            info,
            scratch: Vec::new(),
            skip_frames: 0,
            bitrate_bytes: 0,
            bitrate_frames: 0,
            pending_title: None,
        };
        dec.read_title();
        Ok(dec)
    }

    /// Re-selects the default track after a chained-stream reset.
    fn reselect(&mut self) -> Result<(), EngineError> {
        let sel = select_track(self.format.as_ref(), &self.source)?;
        self.decoder = sel.decoder;
        self.track_id = sel.track_id;
        self.time_base = sel.time_base;
        self.sample_rate = sel.sample_rate;
        self.channels = sel.channels;
        self.info.sample_rate = sel.sample_rate;
        self.info.channels = sel.channels as u16;
        self.info.duration = sel.duration;
        Ok(())
    }

    pub fn info(&self) -> &TrackInfo {
        &self.info
    }

    /// Bitrate announced out-of-band (e.g. the `icy-br` stream header).
    pub fn set_kbps(&mut self, kbps: Option<u32>) {
        self.info.kbps = kbps;
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Decodes the next packet and appends interleaved stereo samples to `out`.
    /// Returns `Ok(false)` at end of stream. Corrupt packets are skipped.
    pub fn decode_next(&mut self, out: &mut Vec<f32>) -> Result<bool, EngineError> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => return Ok(false),
                Err(SymError::ResetRequired) => {
                    // Chained Ogg streams (e.g. Icecast song change): new track, new decoder.
                    self.reselect()?;
                    self.read_title();
                    continue;
                }
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok(false);
                }
                Err(SymError::IoError(e)) => return Err(EngineError::Decode(e.to_string())),
                Err(e) => return Err(EngineError::Decode(e.to_string())),
            };
            self.read_title();
            if packet.track_id != self.track_id {
                continue;
            }
            self.bitrate_bytes += packet.data.len() as u64;
            match self.decoder.decode(&packet) {
                Ok(buf) => {
                    let frames = buf.frames() as u64;
                    self.bitrate_frames += frames;
                    let channels = buf.spec().channels().count().max(1);
                    buf.copy_to_vec_interleaved(&mut self.scratch);
                    let skip = self.skip_frames.min(frames);
                    self.skip_frames -= skip;
                    to_stereo(&self.scratch[skip as usize * channels..], channels, out);
                    return Ok(true);
                }
                Err(SymError::DecodeError(_)) | Err(SymError::IoError(_)) => continue,
                Err(e) => return Err(EngineError::Decode(e.to_string())),
            }
        }
    }

    /// Seeks to `to`; returns the position actually reached.
    pub fn seek(&mut self, to: Duration) -> Result<Duration, EngineError> {
        let time = Time::try_from_secs_f64(to.as_secs_f64()).unwrap_or(Time::ZERO);
        let seeked = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time,
                    track_id: Some(self.track_id),
                },
            )
            .map_err(|e| EngineError::Decode(format!("seek failed: {e}")))?;
        self.decoder.reset();
        self.skip_frames = 0;
        if let Some(tb) = self.time_base {
            let required = tb
                .calc_time(seeked.required_ts)
                .map(time_to_duration)
                .unwrap_or(to);
            let actual = tb
                .calc_time(seeked.actual_ts)
                .map(time_to_duration)
                .unwrap_or(required);
            if required > actual {
                self.skip_frames =
                    ((required - actual).as_secs_f64() * f64::from(self.sample_rate)) as u64;
            }
            return Ok(required);
        }
        Ok(to)
    }

    /// Bitrate over the audio decoded since the last call, once at least one second has passed.
    pub fn take_bitrate(&mut self) -> Option<u32> {
        if self.sample_rate == 0 || self.bitrate_frames < u64::from(self.sample_rate) {
            return None;
        }
        let secs = self.bitrate_frames as f64 / f64::from(self.sample_rate);
        let kbps = (self.bitrate_bytes as f64 * 8.0 / secs / 1000.0).round() as u32;
        self.bitrate_bytes = 0;
        self.bitrate_frames = 0;
        Some(kbps)
    }

    /// In-band title change (chained Ogg streams), if any since the last call.
    pub fn take_title(&mut self) -> Option<String> {
        self.pending_title.take()
    }

    fn read_title(&mut self) {
        let mut metadata = self.format.metadata();
        if metadata.is_latest() && self.pending_title.is_some() {
            return;
        }
        if let Some(rev) = metadata.skip_to_latest()
            && let Some(title) = title_from_tags(&rev.media.tags)
        {
            self.pending_title = Some(title);
        }
    }
}

/// "Artist - Title", or whichever of the two exists.
pub fn title_from_tags(tags: &[Tag]) -> Option<String> {
    let mut artist = None;
    let mut title = None;
    for tag in tags {
        match &tag.std {
            Some(StandardTag::Artist(a)) => artist = Some(a.to_string()),
            Some(StandardTag::TrackTitle(t)) => title = Some(t.to_string()),
            _ => {}
        }
    }
    match (artist, title) {
        (Some(a), Some(t)) => Some(format!("{a} - {t}")),
        (None, Some(t)) => Some(t),
        (Some(a), None) => Some(a),
        (None, None) => None,
    }
}

fn time_to_duration(t: Time) -> Duration {
    Duration::from_secs_f64(t.as_secs_f64().max(0.0))
}

struct SelectedTrack {
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    sample_rate: u32,
    channels: usize,
    duration: Option<Duration>,
}

fn select_track(format: &dyn FormatReader, source: &Source) -> Result<SelectedTrack, EngineError> {
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| EngineError::Unsupported(format!("{source}: no audio track")))?;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| EngineError::Unsupported(format!("{source}: unknown codec")))?;
    let decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| EngineError::Unsupported(format!("{source}: {e}")))?;
    let sample_rate = params.sample_rate.unwrap_or(44_100);
    let channels = params.channels.as_ref().map(|c| c.count()).unwrap_or(2);
    let duration = match (track.num_frames, track.duration, track.time_base) {
        (Some(n), _, _) if sample_rate > 0 => {
            Some(Duration::from_secs_f64(n as f64 / f64::from(sample_rate)))
        }
        (_, Some(d), Some(tb)) => tb.calc_duration(d).map(time_to_duration),
        _ => None,
    };
    Ok(SelectedTrack {
        decoder,
        track_id: track.id,
        time_base: track.time_base,
        sample_rate,
        channels,
        duration,
    })
}
