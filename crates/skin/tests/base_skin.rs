use winrust_skin::{Sheet, Skin, SpriteId, debug, viscolor};

#[test]
fn base_is_winrust_base_not_the_debug_skin() {
    let base = Skin::base();
    let s = base.sprite(SpriteId::MainWindowBackground);
    assert!(
        s.image != &debug::sheets()[Sheet::Main as usize],
        "the base skin is still the debug skin"
    );
    // The time display's colon is part of Winrust Base's main.bmp.
    assert_eq!(s.image.get(72, 29), 0xFF00_E000);
}

#[test]
fn base_has_every_sheet_at_its_size() {
    let base = Skin::base();
    for sheet in Sheet::ALL {
        assert!(base.has_sheet(sheet), "{sheet:?}");
        let img = base.sheet(sheet);
        assert_eq!((img.width, img.height), sheet.size(), "{sheet:?}");
    }
    assert!(base.uses_nums_ex());
}

#[test]
fn base_text_files_are_applied() {
    let base = Skin::base();
    assert_eq!(base.playlist.normal, 0xFF00_E000);
    assert_eq!(base.playlist.selected_bg, 0xFF6B_2A12);
    assert_eq!(base.vis_colors, viscolor::DEFAULT_COLORS);
}
