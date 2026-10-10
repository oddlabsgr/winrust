use winrust_skin::ini::{decode_text, parse as parse_ini};
use winrust_skin::pledit::{self, PlaylistStyle};
use winrust_skin::region::{self, Polygon};
use winrust_skin::viscolor::{self, DEFAULT_COLORS};

// ini

#[test]
fn sections_keys_and_values() {
    let ini = parse_ini(
        "ignored=1\r\n[Text]\r\nNormal = #00FF00\n ; Font=no\nFont=\"Arial\"\nbad line\n",
    );
    let text = &ini["text"];
    assert_eq!(text["normal"], "#00FF00");
    assert_eq!(text["font"], "Arial");
    assert_eq!(text.len(), 2);
    assert_eq!(ini.len(), 1);
}

#[test]
fn value_stops_at_second_equals_and_loses_one_quote_each_side() {
    let ini = parse_ini("[a]\nk='x=y'\nq=\"\"v\"\"\n");
    assert_eq!(ini["a"]["k"], "x");
    assert_eq!(ini["a"]["q"], "\"v\"");
}

#[test]
fn repeated_section_starts_over() {
    let ini = parse_ini("[a]\nx=1\n[A]\ny=2\n");
    assert_eq!(ini["a"].get("x"), None);
    assert_eq!(ini["a"]["y"], "2");
}

#[test]
fn text_decoding() {
    assert_eq!(decode_text(b"\xEF\xBB\xBFabc"), "abc");
    assert_eq!(decode_text("Ärial".as_bytes()), "Ärial");
    assert_eq!(decode_text(b"\xC4rial"), "Ärial");
}

// pledit.txt

#[test]
fn reads_colors_and_font() {
    let style = pledit::parse(
        "[Text]\nNormal=#00FF00\nCurrent=FFFFFF\nNormalBG=#000000\nSelectedBG=#0000C6\nFont=Tahoma\n",
    );
    assert_eq!(
        style,
        PlaylistStyle {
            normal: 0xFF00_FF00,
            current: 0xFFFF_FFFF,
            normal_bg: 0xFF00_0000,
            selected_bg: 0xFF00_00C6,
            font: "Tahoma".into(),
        }
    );
}

#[test]
fn tolerates_junk_and_keeps_defaults() {
    let style = pledit::parse("[text]\nnormal=#12345678\ncurrent=#FFF\nnormalbg=green\nfont=\n");
    assert_eq!(style.normal, 0xFF12_3456);
    assert_eq!(style.current, 0xFFFF_FFFF);
    assert_eq!(style.normal_bg, PlaylistStyle::default().normal_bg);
    assert_eq!(style.font, "Arial");
    assert_eq!(pledit::parse("no sections"), PlaylistStyle::default());
}

// viscolor.txt

#[test]
fn reads_lines_in_order_with_comments() {
    let colors = viscolor::parse("0,0,0, // background\n 24 , 33,41 // dots\n255 255 255\n");
    assert_eq!(colors[0], 0xFF00_0000);
    assert_eq!(colors[1], 0xFF18_2129);
    assert_eq!(colors[2], 0xFFFF_FFFF);
    assert_eq!(colors[3], DEFAULT_COLORS[3]);
}

#[test]
fn non_matching_lines_do_not_use_an_entry() {
    let colors = viscolor::parse("// header\n\n1,2,3\n");
    assert_eq!(colors[0], 0xFF01_0203);
    assert_eq!(colors[1], DEFAULT_COLORS[1]);
}

#[test]
fn regex_quirks_and_clamping() {
    let first = |line: &str| viscolor::parse(line)[0];
    assert_eq!(first("12,34"), 0xFF0C_0304);
    assert_eq!(first("123"), 0xFF01_0203);
    assert_eq!(first("300,0,99999999999"), 0xFFFF_00FF);
    assert_eq!(first("1,2"), DEFAULT_COLORS[0]);
    assert_eq!(first("x 1,2,3"), DEFAULT_COLORS[0]);
}

#[test]
fn extra_lines_are_ignored() {
    let text = "1,1,1\n".repeat(30);
    assert!(viscolor::parse(&text).iter().all(|&c| c == 0xFF01_0101));
}

// region.txt

#[test]
fn reads_sections_with_mixed_separators() {
    let r = region::parse(
        "[Normal]\nNumPoints=4, 3\nPointList=0,0, 275,0 275 116,0,116  1,1,2,2,3,3\n\
         [WindowShade]\nNumPoints=4\nPointList=0,0,275,0,275,14,0,14\n",
    );
    assert_eq!(
        r.normal,
        vec![
            vec![(0, 0), (275, 0), (275, 116), (0, 116)],
            vec![(1, 1), (2, 2), (3, 3)],
        ]
    );
    assert_eq!(r.window_shade.len(), 1);
    assert!(r.equalizer.is_empty());
}

#[test]
fn small_polygons_skip_their_points() {
    let r = region::parse("[Equalizer]\nNumPoints=2,3\nPointList=9,9,9,9,1,1,2,2,3,3\n");
    assert_eq!(r.equalizer, vec![vec![(1, 1), (2, 2), (3, 3)]]);
}

#[test]
fn short_point_list_keeps_partial_then_drops() {
    let r = region::parse("[Normal]\nNumPoints=4,3\nPointList=0,0,1,0,1,1\n");
    assert_eq!(r.normal, vec![vec![(0, 0), (1, 0), (1, 1)]]);
}

#[test]
fn bad_numbers() {
    let r = region::parse("[Normal]\nNumPoints=3,x,3\nPointList=0,0,1,0,1,1,5,5,6,6,7,7\n");
    assert_eq!(
        r.normal,
        vec![vec![(0, 0), (1, 0), (1, 1)], vec![(5, 5), (6, 6), (7, 7)]]
    );
    let r = region::parse("[Normal]\nNumPoints=3,3\nPointList=0,0,1,0,1,z,5,5,6,6,7,7\n");
    assert_eq!(r.normal, vec![vec![(5, 5), (6, 6), (7, 7)]]);
    assert_eq!(
        region::parse("[Normal]\nNumPoints=3\n").normal,
        Vec::<Polygon>::new()
    );
}

#[test]
fn huge_polygon_sizes_do_not_overflow() {
    // Review I2: a size near usize::MAX overflowed the point index (panic in debug/fuzz).
    let r = region::parse("[Normal]\nNumPoints=18446744073709551615,1,3\nPointList=0,0,1,0,1,1\n");
    assert_eq!(r.normal, vec![vec![(0, 0), (1, 0), (1, 1)]]);
}
