use std::fs;
use std::time::{Duration, Instant};
use winrust_app::config::{self, Area, Config, Debounce};
use winrust_app::model::Model;
use winrust_ui::view::TimeMode;
use winrust_ui::vis::VisMode;

fn changed_model() -> Model {
    let mut m = Model::new(1);
    m.volume = 30;
    m.balance = -40;
    m.time_mode = TimeMode::Remaining;
    m.vis_mode = VisMode::Off;
    m.queue.shuffle = true;
    m.queue.repeat = true;
    m.double_size = true;
    m.always_on_top = true;
    m.position = Some((-1200, 40));
    m
}

#[test]
fn settings_survive_a_save_and_a_load() {
    let dir = tempfile::tempdir().unwrap();
    let saved = Config::from_model(&changed_model());
    config::save(dir.path(), &saved).unwrap();
    let (loaded, warning) = config::load(dir.path());
    assert_eq!(warning, None);
    assert_eq!(loaded, saved);
    let mut m = Model::new(1);
    loaded.apply(&mut m);
    assert_eq!((m.volume, m.balance), (30, -40));
    assert_eq!(
        (m.time_mode, m.vis_mode),
        (TimeMode::Remaining, VisMode::Off)
    );
    assert!(m.queue.shuffle && m.queue.repeat && m.double_size && m.always_on_top);
    assert_eq!(m.position, Some((-1200, 40)));
}

#[test]
fn the_file_is_plain_toml() {
    let dir = tempfile::tempdir().unwrap();
    config::save(dir.path(), &Config::default()).unwrap();
    let text = fs::read_to_string(dir.path().join("config.toml")).unwrap();
    for line in [
        "version = 1",
        "volume = 75",
        "time_mode = \"elapsed\"",
        "vis_mode = \"spectrum\"",
    ] {
        assert!(text.contains(line), "{line} missing from:\n{text}");
    }
}

#[test]
fn a_missing_file_gives_defaults_quietly() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(config::load(dir.path()), (Config::default(), None));
}

#[test]
fn a_corrupt_file_is_kept_as_bak_and_reported() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config.toml");
    fs::write(&file, "volume = [").unwrap();
    let (loaded, warning) = config::load(dir.path());
    assert_eq!(loaded, Config::default());
    assert!(warning.unwrap().contains("config.toml"));
    assert!(!file.exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("config.toml.bak")).unwrap(),
        "volume = ["
    );
}

#[test]
fn a_value_of_the_wrong_type_counts_as_corrupt() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.toml"), "volume = \"loud\"\n").unwrap();
    let (loaded, warning) = config::load(dir.path());
    assert_eq!(loaded, Config::default());
    assert!(warning.is_some());
}

#[test]
fn unknown_fields_are_ignored_and_missing_ones_default() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("config.toml"),
        "version = 7\nfuture_setting = true\nvolume = 30\n",
    )
    .unwrap();
    let (loaded, warning) = config::load(dir.path());
    assert_eq!(warning, None);
    assert_eq!(
        loaded,
        Config {
            version: 7,
            volume: 30,
            ..Config::default()
        }
    );
}

#[test]
fn out_of_range_values_are_clamped() {
    let mut m = Model::new(1);
    Config {
        volume: 255,
        balance: -128,
        ..Config::default()
    }
    .apply(&mut m);
    assert_eq!((m.volume, m.balance), (100, -100));
}

#[test]
fn saving_creates_the_folder_and_leaves_no_temporary_file() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("winrust");
    config::save(&nested, &Config::default()).unwrap();
    config::save(&nested, &Config::default()).unwrap();
    let names: Vec<_> = fs::read_dir(&nested)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, vec!["config.toml"]);
}

#[test]
fn saves_wait_a_second_after_the_last_change() {
    let t0 = Instant::now();
    let at = |ms| t0 + Duration::from_millis(ms);
    let mut d = Debounce::default();
    assert_eq!(d.due(), None);
    d.poke(at(0));
    d.poke(at(500));
    assert_eq!(d.due(), Some(at(1500)));
    assert!(!d.take(at(1200)));
    assert!(d.take(at(1500)));
    assert!(!d.take(at(3000)), "saved once");
    d.poke(at(4000));
    assert!(d.take_pending(), "exit flushes");
    assert!(!d.take_pending());
}

const PRIMARY: Area = Area {
    x: 0,
    y: 0,
    w: 1920,
    h: 1080,
};
const LEFT: Area = Area {
    x: -1280,
    y: 0,
    w: 1280,
    h: 1024,
};

#[test]
fn a_window_whose_title_bar_shows_stays_put() {
    let size = (550, 232);
    for pos in [[100, 100], [-1200, 40], [1800, 500]] {
        assert_eq!(
            config::place(Some(pos), size, &[PRIMARY, LEFT], Some(PRIMARY)),
            Some((pos[0], pos[1])),
            "{pos:?}"
        );
    }
}

#[test]
fn a_window_off_every_monitor_moves_to_the_primary_one() {
    let size = (550, 232);
    let centred = Some((685, 424));
    for pos in [[-3000, 100], [100, -50], [1900, 100], [100, 1075]] {
        assert_eq!(
            config::place(Some(pos), size, &[PRIMARY, LEFT], Some(PRIMARY)),
            centred,
            "{pos:?}"
        );
    }
    assert_eq!(
        config::place(None, size, &[PRIMARY], Some(PRIMARY)),
        centred
    );
    assert_eq!(config::place(Some([-3000, 0]), size, &[], None), None);
}
