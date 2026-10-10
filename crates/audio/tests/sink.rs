use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winrust_audio::renderer::{Renderer, Shared};
use winrust_audio::sink::{AudioSink, CaptureSink, output_devices};

#[test]
fn capture_sink_records_rendered_audio() {
    let (mut producer, consumer) = rtrb::RingBuffer::new(64);
    let shared = Arc::new(Shared::new());
    shared.producer_done.store(true, Ordering::Release);
    let (renderer, _analyzer) = Renderer::new(consumer, shared);
    let mut sink = CaptureSink::new(44_100);
    let captured = sink.captured();
    assert_eq!(sink.start(Arc::new(Mutex::new(renderer))).unwrap(), 44_100);

    for s in [0.1f32, 0.2, 0.3, 0.4] {
        producer.push(s).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while captured.lock().unwrap().len() < 4 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    sink.stop();
    assert_eq!(*captured.lock().unwrap(), vec![0.1, 0.2, 0.3, 0.4]);
}

#[test]
fn listing_devices_never_panics() {
    // CI machines have no sound card; this must still return (possibly empty).
    let _ = output_devices();
}
