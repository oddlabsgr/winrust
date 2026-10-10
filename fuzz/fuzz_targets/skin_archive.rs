#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = winrust_skin::Skin::from_zip_bytes(data);
});
