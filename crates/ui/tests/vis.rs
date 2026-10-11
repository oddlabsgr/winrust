use winrust_skin::Image;
use winrust_skin::viscolor::DEFAULT_COLORS as C;
use winrust_ui::vis::{Bandwidth, FRAMES, OscStyle, Vis, VisMode, VisSettings};

fn sine(freq: f32, amp: f32) -> Vec<f32> {
    (0..FRAMES)
        .map(|i| amp * (std::f32::consts::TAU * freq * i as f32 / 44_100.0).sin())
        .collect()
}

fn silence() -> Vec<f32> {
    vec![0.0; FRAMES]
}

fn thin() -> VisSettings {
    VisSettings {
        bandwidth: Bandwidth::Thin,
        ..VisSettings::default()
    }
}

/// Rows of column `x` painted as a bar (normal colouring paints row y in colour y).
fn bar_height(img: &Image, x: u32) -> usize {
    (2..16).filter(|&y| img.get(x, y) == C[y as usize]).count()
}

fn tallest_column(img: &Image) -> u32 {
    (0..75).max_by_key(|&x| bar_height(img, x)).unwrap()
}

fn background(x: u32, y: u32) -> u32 {
    if x.is_multiple_of(2) && !y.is_multiple_of(2) {
        C[1]
    } else {
        C[0]
    }
}

#[test]
fn modes_cycle_and_off_draws_nothing() {
    assert_eq!(VisMode::Spectrum.next(), VisMode::Oscilloscope);
    assert_eq!(VisMode::Oscilloscope.next(), VisMode::Off);
    assert_eq!(VisMode::Off.next(), VisMode::Spectrum);
    let mut vis = Vis::new(VisSettings {
        mode: VisMode::Off,
        ..VisSettings::default()
    });
    assert!(vis.frame(&silence(), &silence(), &C, false).is_none());
}

#[test]
fn silence_draws_only_the_dotted_background() {
    let mut vis = Vis::new(VisSettings::default());
    let img = vis.frame(&silence(), &silence(), &C, false).unwrap();
    assert_eq!((img.width, img.height), (76, 16));
    for y in 0..16 {
        for x in 0..76 {
            assert_eq!(img.get(x, y), background(x, y), "({x},{y})");
        }
    }
}

#[test]
fn a_full_scale_sine_fills_its_column() {
    let mut vis = Vis::new(thin());
    let s = sine(1000.0, 1.0);
    let img = vis.frame(&s, &s, &C, false).unwrap();
    // 15 rows less Webamp's 2-row push-down: rows 3 to 15, row y in colour y.
    let x = tallest_column(&img);
    assert_eq!(bar_height(&img, x), 13);
    for y in 3..16 {
        assert_eq!(img.get(x, y), C[y as usize], "row {y}");
    }
    // Far above 1 kHz there is nothing.
    assert_eq!(bar_height(&img, 74), 0);
}

#[test]
fn bars_fall_three_quarters_of_a_row_per_frame() {
    // Moderate falloff is 12/16 rows per frame; heights round (half up) and lose 2 rows.
    let mut vis = Vis::new(thin());
    let s = sine(1000.0, 1.0);
    let x = tallest_column(&vis.frame(&s, &s, &C, false).unwrap());
    let heights: Vec<usize> = (0..4)
        .map(|_| bar_height(&vis.frame(&silence(), &silence(), &C, false).unwrap(), x))
        .collect();
    assert_eq!(heights, [12, 12, 11, 10]);
}

#[test]
fn peaks_sit_above_the_bars_and_fall_after_them() {
    let mut vis = Vis::new(thin());
    let s = sine(1000.0, 1.0);
    let img = vis.frame(&s, &s, &C, false).unwrap();
    let x = tallest_column(&img);
    assert_eq!(img.get(x, 2), C[23]);
    let rows: Vec<Option<u32>> = (0..80)
        .map(|_| {
            let img = vis.frame(&silence(), &silence(), &C, false).unwrap();
            (0..16).find(|&y| img.get(x, y) == C[23])
        })
        .collect();
    assert_eq!(rows[0], Some(3));
    // The peak only ever falls, then leaves the display.
    for pair in rows.windows(2) {
        match pair {
            [Some(a), Some(b)] => assert!(b >= a, "{rows:?}"),
            [None, Some(_)] => panic!("peak came back: {rows:?}"),
            _ => {}
        }
    }
    assert_eq!(rows[79], None);
}

#[test]
fn peaks_can_be_turned_off() {
    let mut vis = Vis::new(VisSettings {
        peaks: false,
        ..thin()
    });
    let s = sine(1000.0, 1.0);
    let img = vis.frame(&s, &s, &C, false).unwrap();
    assert!(!img.pixels.contains(&C[23]));
}

#[test]
fn wide_bars_come_in_threes_with_a_gap() {
    // A loud chord lights many bands.
    let chord: Vec<f32> = (0..FRAMES)
        .map(|i| {
            [110.0, 440.0, 1760.0, 7040.0]
                .iter()
                .map(|f| 0.25 * (std::f32::consts::TAU * f * i as f32 / 44_100.0).sin())
                .sum()
        })
        .collect();
    let mut vis = Vis::new(VisSettings::default());
    let img = vis.frame(&chord, &chord, &C, false).unwrap();
    let mut lit = 0;
    for group in 0..18 {
        let x = group * 4;
        let h = bar_height(&img, x);
        assert_eq!(bar_height(&img, x + 1), h, "group {group}");
        assert_eq!(bar_height(&img, x + 2), h, "group {group}");
        for y in 0..16 {
            assert_eq!(img.get(x + 3, y), background(x + 3, y), "gap {group}");
        }
        lit += usize::from(h > 0);
    }
    assert!(lit >= 4, "{lit} groups lit");
}

#[test]
fn the_oscilloscope_draws_silence_through_the_middle() {
    let mut vis = Vis::new(VisSettings {
        mode: VisMode::Oscilloscope,
        ..VisSettings::default()
    });
    let img = vis.frame(&silence(), &silence(), &C, false).unwrap();
    // Byte 128 → row round(128 / 8) - 9 = 7, pushed down 2 rows, in scope colour 0.
    for x in 0..76 {
        for y in 0..16 {
            let expected = if y == 9 && x < 75 {
                C[18]
            } else {
                background(x, y)
            };
            assert_eq!(img.get(x, y), expected, "({x},{y})");
        }
    }
}

#[test]
fn a_solid_scope_fills_from_the_middle() {
    let mut vis = Vis::new(VisSettings {
        mode: VisMode::Oscilloscope,
        osc_style: OscStyle::Solid,
        ..VisSettings::default()
    });
    // Full scale clips to row 15: rows 8-15, pushed down 2 and cut at the bottom.
    let high = vec![1.0; FRAMES];
    let img = vis.frame(&high, &high, &C, false).unwrap();
    for y in 0..16 {
        let expected = if y >= 10 { C[22] } else { background(10, y) };
        assert_eq!(img.get(10, y), expected, "row {y}");
    }
}

#[test]
fn shade_mode_draws_a_38_by_5_strip() {
    let mut vis = Vis::new(VisSettings::default());
    let img = vis.frame(&silence(), &silence(), &C, true).unwrap();
    assert_eq!((img.width, img.height), (38, 5));
    // No dots; Webamp keeps the small display's zero peaks on its bottom row.
    for x in 0..38 {
        for y in 0..5 {
            let peak = y == 4 && x < 37 && x % 4 != 3;
            assert_eq!(img.get(x, y), if peak { C[23] } else { C[0] }, "({x},{y})");
        }
    }
}

#[test]
fn short_or_broken_input_draws_without_panicking() {
    let mut vis = Vis::new(VisSettings::default());
    // Before the tap fills up, or if a decoder misbehaves.
    assert!(vis.frame(&[], &[], &C, false).is_some());
    let broken = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1e30];
    assert!(vis.frame(&broken, &broken, &C, false).is_some());
    let mut scope = Vis::new(VisSettings {
        mode: VisMode::Oscilloscope,
        ..VisSettings::default()
    });
    assert!(scope.frame(&broken, &[], &C, true).is_some());
}
