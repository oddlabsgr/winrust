#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Read;
use winrust_audio::icy::{IcyReader, parse_stream_title};

// First byte picks the metadata interval; the rest is the stream.
fuzz_target!(|data: &[u8]| {
    let Some((&interval, stream)) = data.split_first() else {
        return;
    };
    let (titles, _rx) = crossbeam_channel::unbounded();
    let mut reader = IcyReader::new(stream, usize::from(interval) + 1, titles);
    let _ = reader.read_to_end(&mut Vec::new());
    let _ = parse_stream_title(stream);
});
