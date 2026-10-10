//! Damaged bitmaps and archives must load or fail cleanly, never panic. (Deeper fuzzing
//! runs nightly in CI: see `fuzz/`.)

mod common;

use common::*;
use winrust_skin::{Image, Skin, bmp};

/// xorshift64: deterministic and dependency-free.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn mutate(rng: &mut Rng, data: &[u8]) -> Vec<u8> {
    let mut out = data.to_vec();
    for _ in 0..1 + rng.below(8) {
        let i = rng.below(out.len());
        out[i] = rng.next() as u8;
    }
    if rng.below(4) == 0 {
        out.truncate(rng.below(out.len() + 1));
    }
    out
}

#[test]
fn damaged_bitmaps_never_panic() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let mut img = Image::new(7, 5, 0xFF12_3456);
    img.set(3, 2, 0xFFFF_FFFF);
    let seed = bmp::encode24(&img);
    for _ in 0..5000 {
        let data = mutate(&mut rng, &seed);
        let _ = bmp::decode_any(&data);
    }
}

#[test]
fn damaged_archives_never_panic() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let seed = zip(&[
        ("main.bmp", &solid_bmp(8, 8, 0xFF00_FF00)),
        ("pledit.txt", b"[Text]\nNormal=#00FF00\n"),
        (
            "region.txt",
            b"[Normal]\nNumPoints=3\nPointList=0,0,1,0,1,1\n",
        ),
        ("viscolor.txt", b"1,2,3\n"),
    ]);
    for _ in 0..1000 {
        let data = mutate(&mut rng, &seed);
        let _ = Skin::from_zip_bytes(&data);
    }
}
