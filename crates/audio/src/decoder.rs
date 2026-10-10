//! symphonia wrapper: opens a source and yields interleaved stereo f32 at the source rate.

use crate::dsp::to_stereo;
use crate::mp4::{self, Gapless};
use crate::types::{EngineError, Source, TrackInfo};
use std::fs::File;
use std::path::Path;
use std::time::Duration;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::{Error as SymError, SeekErrorKind};
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
    /// Encoder priming frames still to drop (MP4 edit list or iTunSMPB).
    trim_skip: u64,
    /// Real audio frames left before the encoder padding starts, when known.
    trim_left: Option<u64>,
    /// MP4 gapless info, kept for seeking.
    gapless: Option<Gapless>,
    /// A seek landed at or past the end: report end of stream until the next seek.
    at_end: bool,
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
        // symphonia does not apply MP4 edit lists, so AAC priming/padding is trimmed here.
        let gapless = File::open(path)
            .ok()
            .and_then(|mut f| mp4::read_gapless(&mut f, dec.sample_rate));
        if let Some(g) = gapless {
            dec.apply_gapless(g);
        }
        if let (Some(len), Some(d)) = (byte_len, dec.info.duration)
            && d.as_secs_f64() > 0.0
        {
            dec.info.kbps = Some((len as f64 * 8.0 / d.as_secs_f64() / 1000.0).round() as u32);
        }
        Ok(dec)
    }

    fn apply_gapless(&mut self, g: Gapless) {
        self.gapless = Some(g);
        self.trim_skip = g.delay;
        self.trim_left = g.valid;
        if let Some(valid) = g.valid {
            self.info.duration = Some(Duration::from_secs_f64(
                valid as f64 / f64::from(self.sample_rate),
            ));
        }
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
            trim_skip: 0,
            trim_left: None,
            gapless: None,
            at_end: false,
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
            if self.at_end || self.trim_left == Some(0) {
                // Seeked past the end, or the rest is encoder padding.
                return Ok(false);
            }
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
                    let mut start = self.skip_frames.min(frames);
                    self.skip_frames -= start;
                    let priming = self.trim_skip.min(frames - start);
                    self.trim_skip -= priming;
                    start += priming;
                    let mut end = frames;
                    if let Some(left) = self.trim_left.as_mut() {
                        end = start + (end - start).min(*left);
                        *left -= end - start;
                    }
                    let (start, end) = (start as usize * channels, end as usize * channels);
                    to_stereo(&self.scratch[start..end], channels, out);
                    return Ok(true);
                }
                Err(SymError::DecodeError(_)) | Err(SymError::IoError(_)) => continue,
                Err(e) => return Err(EngineError::Decode(e.to_string())),
            }
        }
    }

    /// Seeks to `to`; returns the position actually reached.
    pub fn seek(&mut self, to: Duration) -> Result<Duration, EngineError> {
        // With MP4 priming, audio time `to` sits `delay` later in the media timeline.
        let rate = f64::from(self.sample_rate.max(1));
        let delay = self
            .gapless
            .map(|g| Duration::from_secs_f64(g.delay as f64 / rate))
            .unwrap_or_default();
        self.at_end = false;
        if let Some(duration) = self.info.duration
            && to >= duration
        {
            self.at_end = true;
            return Ok(duration);
        }
        let target = to + delay;
        let time = Time::try_from_secs_f64(target.as_secs_f64()).unwrap_or(Time::ZERO);
        let seeked = match self.format.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time,
                track_id: Some(self.track_id),
            },
        ) {
            Ok(seeked) => seeked,
            // Unknown or overstated duration: past the end is still just the end.
            Err(SymError::SeekError(SeekErrorKind::OutOfRange)) => {
                self.at_end = true;
                return Ok(self.info.duration.unwrap_or(to));
            }
            Err(e) => return Err(EngineError::Decode(format!("seek failed: {e}"))),
        };
        self.decoder.reset();
        self.skip_frames = 0;
        let mut required = target;
        if let Some(tb) = self.time_base {
            required = tb
                .calc_time(seeked.required_ts)
                .map(time_to_duration)
                .unwrap_or(target);
            let actual = tb
                .calc_time(seeked.actual_ts)
                .map(time_to_duration)
                .unwrap_or(required);
            if required > actual {
                self.skip_frames = ((required - actual).as_secs_f64() * rate) as u64;
            }
        }
        let position = required.saturating_sub(delay);
        self.trim_skip = 0;
        self.trim_left = self
            .gapless
            .and_then(|g| g.valid)
            .map(|valid| valid.saturating_sub((position.as_secs_f64() * rate).round() as u64));
        Ok(position)
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
    let sample_rate = match params.sample_rate {
        Some(0) => {
            return Err(EngineError::Unsupported(format!(
                "{source}: invalid sample rate 0"
            )));
        }
        Some(rate) => rate,
        None => 44_100,
    };
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
