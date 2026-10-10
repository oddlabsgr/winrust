//! Audio outputs. `CpalSink` drives a real device; `CaptureSink` records for tests.

use crate::renderer::Renderer;
use crate::types::EngineError;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, ErrorKind, FromSample, SampleFormat, SizedSample, StreamConfig};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, bounded, unbounded};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// The renderer is locked only with `try_lock` from the callback, so it never blocks there.
pub type SharedRenderer = Arc<Mutex<Renderer>>;

#[derive(Clone, Debug, PartialEq)]
pub enum SinkEvent {
    /// The device was reopened at a different rate; the engine must reconfigure.
    RateChanged(u32),
    Error(String),
}

pub trait AudioSink: Send {
    /// Starts pulling audio from `renderer`. Returns the device sample rate.
    fn start(&mut self, renderer: SharedRenderer) -> Result<u32, EngineError>;
    /// Switches to the named device (`None` = system default). Returns the new rate.
    fn set_device(&mut self, name: Option<String>) -> Result<u32, EngineError>;
    /// Problems noticed since the last call. Never blocks.
    fn poll(&mut self) -> Option<SinkEvent>;
    fn stop(&mut self);
}

/// Names of all output devices on the default host.
pub fn output_devices() -> Vec<String> {
    let host = cpal::default_host();
    let Ok(devices) = host.output_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|d| d.description().ok().map(|desc| desc.name().to_string()))
        .collect()
}

// ---------------------------------------------------------------------------------------------

/// Test output: records only real (non-silence) frames, as interleaved stereo.
pub struct CaptureSink {
    rate: u32,
    realtime: bool,
    captured: Arc<Mutex<Vec<f32>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl CaptureSink {
    /// Pulls audio as fast as the engine can produce it.
    pub fn new(rate: u32) -> Self {
        CaptureSink {
            rate,
            realtime: false,
            captured: Arc::default(),
            stop: Arc::default(),
            thread: None,
        }
    }

    /// Pulls audio at the speed a real device would.
    pub fn realtime(rate: u32) -> Self {
        let mut sink = Self::new(rate);
        sink.realtime = true;
        sink
    }

    /// Interleaved stereo captured so far.
    pub fn captured(&self) -> Arc<Mutex<Vec<f32>>> {
        self.captured.clone()
    }
}

impl AudioSink for CaptureSink {
    fn start(&mut self, renderer: SharedRenderer) -> Result<u32, EngineError> {
        let captured = self.captured.clone();
        let stop = self.stop.clone();
        let period = if self.realtime {
            Duration::from_secs_f64(512.0 / f64::from(self.rate))
        } else {
            Duration::from_micros(500)
        };
        self.thread = Some(thread::spawn(move || {
            let mut buf = [0.0f32; 1024];
            while !stop.load(Ordering::Relaxed) {
                let n = renderer
                    .lock()
                    .map(|mut r| r.render(&mut buf, 2))
                    .unwrap_or(0);
                if n > 0 {
                    captured.lock().unwrap().extend_from_slice(&buf[..n * 2]);
                }
                thread::sleep(period);
            }
        }));
        Ok(self.rate)
    }

    fn set_device(&mut self, _name: Option<String>) -> Result<u32, EngineError> {
        Ok(self.rate)
    }

    fn poll(&mut self) -> Option<SinkEvent> {
        None
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for CaptureSink {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---------------------------------------------------------------------------------------------

/// Real output device. A dedicated thread owns the cpal stream (streams are not `Send` on
/// every platform) and rebuilds it when the device disappears.
pub struct CpalSink {
    device: Option<String>,
    rate: u32,
    renderer: Option<SharedRenderer>,
    events_tx: Sender<SinkEvent>,
    events_rx: Receiver<SinkEvent>,
    worker: Option<(Sender<()>, JoinHandle<()>)>,
}

impl CpalSink {
    /// `device`: output device name, `None` for the system default.
    pub fn new(device: Option<String>) -> Self {
        let (events_tx, events_rx) = unbounded();
        CpalSink {
            device,
            rate: 0,
            renderer: None,
            events_tx,
            events_rx,
            worker: None,
        }
    }

    fn launch(&mut self, preferred_rate: Option<u32>) -> Result<u32, EngineError> {
        self.stop();
        let renderer = self
            .renderer
            .clone()
            .ok_or_else(|| EngineError::Device("sink not started".into()))?;
        let (ready_tx, ready_rx) = bounded(1);
        let (stop_tx, stop_rx) = bounded(1);
        let name = self.device.clone();
        let events = self.events_tx.clone();
        let handle = thread::Builder::new()
            .name("winrust-output".into())
            .spawn(move || device_thread(name, renderer, preferred_rate, ready_tx, stop_rx, events))
            .map_err(|e| EngineError::Device(e.to_string()))?;
        self.worker = Some((stop_tx, handle));
        let rate = ready_rx
            .recv()
            .map_err(|_| EngineError::Device("output thread died".into()))??;
        self.rate = rate;
        Ok(rate)
    }
}

impl AudioSink for CpalSink {
    fn start(&mut self, renderer: SharedRenderer) -> Result<u32, EngineError> {
        self.renderer = Some(renderer);
        self.launch(None)
    }

    fn set_device(&mut self, name: Option<String>) -> Result<u32, EngineError> {
        self.device = name;
        let rate = self.rate;
        self.launch(Some(rate))
    }

    fn poll(&mut self) -> Option<SinkEvent> {
        match self.events_rx.try_recv().ok()? {
            SinkEvent::RateChanged(r) => {
                self.rate = r;
                Some(SinkEvent::RateChanged(r))
            }
            other => Some(other),
        }
    }

    fn stop(&mut self) {
        if let Some((stop, handle)) = self.worker.take() {
            let _ = stop.send(());
            let _ = handle.join();
        }
    }
}

impl Drop for CpalSink {
    fn drop(&mut self) {
        self.stop();
    }
}

fn device_thread(
    name: Option<String>,
    renderer: SharedRenderer,
    mut preferred_rate: Option<u32>,
    ready: Sender<Result<u32, EngineError>>,
    stop: Receiver<()>,
    events: Sender<SinkEvent>,
) {
    let mut first = true;
    // An outage is reported once, and only when no output can be opened at all.
    let mut outage_reported = false;
    loop {
        let lost = Arc::new(AtomicBool::new(false));
        match open_stream(name.as_deref(), &renderer, preferred_rate, lost.clone()) {
            Ok((stream, rate)) => {
                if first {
                    let _ = ready.send(Ok(rate));
                    first = false;
                } else if Some(rate) != preferred_rate {
                    let _ = events.send(SinkEvent::RateChanged(rate));
                }
                preferred_rate = Some(rate);
                outage_reported = false;
                loop {
                    match stop.recv_timeout(Duration::from_millis(100)) {
                        Ok(()) | Err(RecvTimeoutError::Disconnected) => return,
                        Err(RecvTimeoutError::Timeout) if lost.load(Ordering::Relaxed) => break,
                        Err(RecvTimeoutError::Timeout) => {}
                    }
                }
                // Reopened quietly on the next turn (spec §6.7): not an error for the user.
                drop(stream);
            }
            Err(e) => {
                if first {
                    let _ = ready.send(Err(e));
                    return;
                }
                // Device gone (e.g. unplugged): tell the user once, retry every second.
                if !outage_reported {
                    outage_reported = true;
                    let _ = events.send(SinkEvent::Error(format!("no audio output: {e}")));
                }
                match stop.recv_timeout(Duration::from_secs(1)) {
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => return,
                    Err(RecvTimeoutError::Timeout) => {}
                }
            }
        }
    }
}

fn open_stream(
    name: Option<&str>,
    renderer: &SharedRenderer,
    preferred_rate: Option<u32>,
    lost: Arc<AtomicBool>,
) -> Result<(cpal::Stream, u32), EngineError> {
    let host = cpal::default_host();
    let named = name.and_then(|n| {
        host.output_devices().ok()?.find(|d| {
            d.description()
                .map(|desc| desc.name() == n)
                .unwrap_or(false)
        })
    });
    let device = named
        .or_else(|| host.default_output_device())
        .ok_or_else(|| EngineError::Device("no output device".into()))?;
    let default = device
        .default_output_config()
        .map_err(|e| EngineError::Device(e.to_string()))?;
    let channels = default.channels();
    let format = default.sample_format();
    let mut rate = default.sample_rate();
    if let Some(want) = preferred_rate {
        let supported = device.supported_output_configs().map(|mut configs| {
            configs.any(|c| {
                c.channels() == channels
                    && c.sample_format() == format
                    && c.min_sample_rate() <= want
                    && want <= c.max_sample_rate()
            })
        });
        if supported.unwrap_or(false) {
            rate = want;
        }
    }
    let config = StreamConfig {
        channels,
        sample_rate: rate,
        buffer_size: BufferSize::Default,
    };
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, config, renderer, lost),
        SampleFormat::I16 => build::<i16>(&device, config, renderer, lost),
        SampleFormat::U16 => build::<u16>(&device, config, renderer, lost),
        SampleFormat::I32 => build::<i32>(&device, config, renderer, lost),
        SampleFormat::F64 => build::<f64>(&device, config, renderer, lost),
        SampleFormat::I8 => build::<i8>(&device, config, renderer, lost),
        SampleFormat::U8 => build::<u8>(&device, config, renderer, lost),
        other => Err(EngineError::Device(format!(
            "unsupported sample format {other}"
        ))),
    }?;
    stream
        .play()
        .map_err(|e| EngineError::Device(e.to_string()))?;
    Ok((stream, rate))
}

/// Stream errors that leave the stream dead. A `DeviceChanged` stream was rerouted by the
/// backend and keeps playing; xruns and refused real-time priority are not fatal either.
fn needs_rebuild(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::DeviceNotAvailable
            | ErrorKind::StreamInvalidated
            | ErrorKind::HostUnavailable
            | ErrorKind::DeviceBusy
    )
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    renderer: &SharedRenderer,
    lost: Arc<AtomicBool>,
) -> Result<cpal::Stream, EngineError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels);
    let renderer = renderer.clone();
    let mut scratch = vec![0.0f32; 16_384];
    device
        .build_output_stream::<T, _, _>(
            config,
            move |data: &mut [T], _| {
                if scratch.len() < data.len() {
                    scratch.resize(data.len(), 0.0);
                }
                let buf = &mut scratch[..data.len()];
                match renderer.try_lock() {
                    Ok(mut r) => {
                        r.render(buf, channels);
                    }
                    Err(_) => buf.fill(0.0),
                }
                for (d, s) in data.iter_mut().zip(buf.iter()) {
                    *d = T::from_sample(*s);
                }
            },
            move |err: cpal::Error| {
                if needs_rebuild(err.kind()) {
                    lost.store(true, Ordering::Relaxed);
                }
            },
            None,
        )
        .map_err(|e| EngineError::Device(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_dead_streams_are_rebuilt() {
        // Review I3: a rerouted default-device stream keeps playing; tearing it down caused a
        // needless gap and a spurious error on every headphone plug.
        assert!(!needs_rebuild(ErrorKind::DeviceChanged));
        assert!(!needs_rebuild(ErrorKind::Xrun));
        assert!(!needs_rebuild(ErrorKind::RealtimeDenied));
        assert!(needs_rebuild(ErrorKind::DeviceNotAvailable));
        assert!(needs_rebuild(ErrorKind::StreamInvalidated));
        assert!(needs_rebuild(ErrorKind::HostUnavailable));
    }
}
