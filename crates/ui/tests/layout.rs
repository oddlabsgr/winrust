use winrust_skin::Rect;
use winrust_ui::layout::{self, Kind, Mode, Widget};

#[test]
fn widgets_sit_at_webamps_positions() {
    let cases = [
        (Mode::Normal, Widget::Close, Rect::new(264, 3, 9, 9)),
        (Mode::Normal, Widget::Time, Rect::new(39, 26, 59, 13)),
        (Mode::Normal, Widget::Volume, Rect::new(107, 57, 68, 13)),
        (Mode::Normal, Widget::Next, Rect::new(108, 88, 22, 18)),
        (Mode::Normal, Widget::Eject, Rect::new(136, 89, 22, 16)),
        (Mode::Shade, Widget::Seek, Rect::new(226, 4, 17, 7)),
        (Mode::Shade, Widget::Vis, Rect::new(79, 5, 38, 5)),
    ];
    for (mode, widget, r) in cases {
        assert_eq!(layout::rect(mode, widget), Some(r), "{mode:?} {widget:?}");
    }
    assert_eq!(layout::rect(Mode::Shade, Widget::Volume), None);
}

#[test]
fn every_widget_lies_inside_its_window() {
    for mode in [Mode::Normal, Mode::Shade] {
        let (w, h) = mode.size();
        for &(widget, r) in layout::widgets(mode) {
            assert!(r.x + r.w <= w && r.y + r.h <= h, "{mode:?} {widget:?}");
        }
    }
}

#[test]
fn hit_test_finds_the_topmost_widget() {
    let n = Mode::Normal;
    assert_eq!(layout::hit_test(n, 268, 7), Some(Widget::Close));
    assert_eq!(layout::hit_test(n, 100, 7), Some(Widget::TitleBar));
    assert_eq!(layout::hit_test(n, 50, 95), Some(Widget::Play));
    // Shuffle (x 164-210) and repeat (x 210-237) overlap at x = 210; repeat is on top.
    assert_eq!(layout::hit_test(n, 210, 95), Some(Widget::Repeat));
    assert_eq!(layout::hit_test(n, 209, 95), Some(Widget::Shuffle));
    // Between next and eject.
    assert_eq!(layout::hit_test(n, 132, 95), None);
    assert_eq!(layout::hit_test(n, -1, 5), None);
    assert_eq!(layout::hit_test(n, 275, 5), None);
    let s = Mode::Shade;
    assert_eq!(layout::hit_test(s, 180, 6), Some(Widget::Play));
    assert_eq!(layout::hit_test(s, 100, 2), Some(Widget::TitleBar));
    assert_eq!(layout::hit_test(s, 50, 50), None);
}

#[test]
fn widgets_know_how_they_take_the_pointer() {
    assert_eq!(Widget::Volume.kind(), Kind::Slider);
    assert_eq!(Widget::Seek.kind(), Kind::Slider);
    assert_eq!(Widget::TitleBar.kind(), Kind::Drag);
    assert_eq!(Widget::Marquee.kind(), Kind::Drag);
    assert_eq!(Widget::Play.kind(), Kind::Button);
    assert_eq!(Widget::Time.kind(), Kind::Button);
    assert_eq!(layout::thumb_width(Mode::Normal, Widget::Seek), 29);
    assert_eq!(layout::thumb_width(Mode::Shade, Widget::Seek), 3);
    assert_eq!(layout::thumb_width(Mode::Normal, Widget::Balance), 14);
}
