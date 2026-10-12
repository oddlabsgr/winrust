#![no_main]

use libfuzzer_sys::fuzz_target;
use std::path::Path;
use winrust_playlist::entry::Tags;
use winrust_playlist::files::{decode, parse_m3u, parse_pls};
use winrust_playlist::title::Template;

// The same bytes as an M3U file, a PLS file and a title template.
fuzz_target!(|data: &[u8]| {
    let text = decode(data, true);
    let _ = parse_m3u(&text, Path::new("base"));
    let _ = parse_pls(&text, Path::new("base"));
    let _ = Template::parse(&text).render(&Tags::default(), "name");
});
