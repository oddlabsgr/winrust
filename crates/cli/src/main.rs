//! `winrust-cli`: headless player for exercising the audio engine from a terminal.

mod queue;

use clap::{Args, Parser, Subcommand};
use crossbeam_channel::{select, unbounded};
use queue::{Next, Queue};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;
use winrust_audio::{Command, CpalSink, Engine, EngineEvent, EqSettings, Source, output_devices};

#[derive(Parser)]
#[command(
    name = "winrust-cli",
    version,
    about = "Headless Winrust player (audio engine test tool)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List audio output devices.
    Devices,
    /// Play files and stream URLs in order, gaplessly.
    Play(PlayArgs),
}

#[derive(Args)]
struct PlayArgs {
    /// Files or http(s):// stream URLs.
    #[arg(required = true)]
    sources: Vec<String>,
    /// Volume slider, 0..=100.
    #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(u8).range(0..=100))]
    volume: u8,
    /// Balance, -1.0 (left) ..= 1.0 (right).
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    balance: f32,
    /// Ten comma-separated band gains in dB (60 Hz .. 16 kHz). Enables the equalizer.
    #[arg(long, value_delimiter = ',', allow_hyphen_values = true)]
    eq: Option<Vec<f32>>,
    /// Equalizer preamp in dB.
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    preamp: f32,
    /// Start the first track at this many seconds.
    #[arg(long)]
    start: Option<f64>,
    /// Output device name (see `devices`).
    #[arg(long)]
    device: Option<String>,
}

fn main() -> ExitCode {
    match Cli::parse().cmd {
        Cmd::Devices => {
            for name in output_devices() {
                println!("{name}");
            }
            ExitCode::SUCCESS
        }
        Cmd::Play(args) => match play(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(msg) => {
                eprintln!("error: {msg}");
                ExitCode::FAILURE
            }
        },
    }
}

fn parse_source(s: &str) -> Source {
    let lower = s.to_ascii_lowercase();
    if ["http://", "https://", "icy://"]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        Source::Url(s.to_string())
    } else {
        Source::File(PathBuf::from(s))
    }
}

fn eq_settings(args: &PlayArgs) -> Result<EqSettings, String> {
    let Some(bands) = &args.eq else {
        return Ok(EqSettings::default());
    };
    let bands_db: [f32; 10] = bands
        .as_slice()
        .try_into()
        .map_err(|_| format!("--eq needs 10 values, got {}", bands.len()))?;
    Ok(EqSettings {
        enabled: true,
        preamp_db: args.preamp,
        bands_db,
    })
}

fn fmt_time(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}

fn play(args: PlayArgs) -> Result<(), String> {
    let sources: Vec<Source> = args.sources.iter().map(|s| parse_source(s)).collect();
    let eq = eq_settings(&args)?;
    let engine =
        Engine::spawn(Box::new(CpalSink::new(args.device.clone()))).map_err(|e| e.to_string())?;
    let h = engine.handle();
    let mut volume = args.volume;
    h.send(Command::SetVolume(volume));
    h.send(Command::SetBalance(args.balance));
    h.send(Command::SetEq(eq));

    let (lines_tx, lines) = unbounded();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if lines_tx.send(line).is_err() {
                break;
            }
        }
    });
    println!("keys + Enter: p=pause/resume  s=stop  n=next  f/b=seek ±5s  +/-=volume  q=quit");

    let mut queue = Queue::new(sources.len());
    let mut position = Duration::ZERO;
    let mut last_print = Duration::MAX;
    let mut paused = false;
    let mut pending_start = args.start.map(Duration::from_secs_f64);
    let act = |next: Next| -> bool {
        match next {
            Next::Load(i) => h.send(Command::Load(sources[i].clone())),
            Next::Enqueue(i) => h.send(Command::QueueNext(sources[i].clone())),
            Next::Quit => return false,
            Next::Nothing => {}
        }
        true
    };
    if !act(queue.start()) {
        return Ok(());
    }

    loop {
        select! {
            recv(engine.events()) -> ev => {
                let Ok(ev) = ev else { return Ok(()) };
                let next = match ev {
                    EngineEvent::TrackStarted { info, gapless } => {
                        let khz = f64::from(info.sample_rate) / 1000.0;
                        let kbps = info.kbps.map(|k| format!("{k} kbps, ")).unwrap_or_default();
                        let len = info.duration.map(fmt_time).unwrap_or_else(|| "live".into());
                        println!("\n▶ {} [{kbps}{khz} kHz, {} ch, {len}]", info.source, info.channels);
                        if let Some(at) = pending_start.take() {
                            h.send(Command::Seek(at));
                        }
                        queue.on_started(gapless)
                    }
                    EngineEvent::Position(p) => {
                        position = p;
                        if p.as_secs() != last_print.as_secs() {
                            last_print = p;
                            print!("\r  {}   ", fmt_time(p));
                            let _ = std::io::stdout().flush();
                        }
                        Next::Nothing
                    }
                    EngineEvent::StreamTitle(t) => {
                        println!("\n  ♪ {t}");
                        Next::Nothing
                    }
                    EngineEvent::Bitrate(_) => Next::Nothing,
                    EngineEvent::State(s) => {
                        println!("\n  [{s:?}]");
                        Next::Nothing
                    }
                    EngineEvent::Error { source, error } => {
                        println!("\n  ✖ {error}");
                        queue.on_error(|i| source.as_ref() == Some(&sources[i]))
                    }
                    EngineEvent::TrackEnded => queue.on_ended(),
                };
                if !act(next) {
                    return Ok(());
                }
            }
            recv(lines) -> line => {
                let Ok(line) = line else { return Ok(()) };
                match line.trim() {
                    "p" => {
                        paused = !paused;
                        h.send(if paused { Command::Pause } else { Command::Play });
                    }
                    "s" => h.send(Command::Stop),
                    "n" => {
                        if !act(queue.skip()) {
                            return Ok(());
                        }
                    }
                    "f" => h.send(Command::Seek(position + Duration::from_secs(5))),
                    "b" => h.send(Command::Seek(position.saturating_sub(Duration::from_secs(5)))),
                    "+" => {
                        volume = (volume + 5).min(100);
                        h.send(Command::SetVolume(volume));
                    }
                    "-" => {
                        volume = volume.saturating_sub(5);
                        h.send(Command::SetVolume(volume));
                    }
                    "q" => return Ok(()),
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_and_files_are_told_apart() {
        assert_eq!(
            parse_source("HTTPS://radio.example/live"),
            Source::Url("HTTPS://radio.example/live".into())
        );
        assert_eq!(
            parse_source("C:\\music\\a.mp3"),
            Source::File(PathBuf::from("C:\\music\\a.mp3"))
        );
    }

    #[test]
    fn eq_needs_ten_bands() {
        let cli = Cli::parse_from(["winrust-cli", "play", "a.mp3", "--eq", "1,2,3"]);
        let Cmd::Play(args) = cli.cmd else { panic!() };
        assert!(eq_settings(&args).unwrap_err().contains("10 values"));
    }

    #[test]
    fn negative_values_parse() {
        let cli = Cli::parse_from([
            "winrust-cli",
            "play",
            "a.mp3",
            "--balance",
            "-0.5",
            "--preamp",
            "-3",
            "--eq",
            "-1,0,0,0,0,0,0,0,0,12",
        ]);
        let Cmd::Play(args) = cli.cmd else { panic!() };
        assert_eq!(args.balance, -0.5);
        let eq = eq_settings(&args).unwrap();
        assert!(eq.enabled);
        assert_eq!(eq.preamp_db, -3.0);
        assert_eq!(eq.bands_db[0], -1.0);
        assert_eq!(eq.bands_db[9], 12.0);
    }

    #[test]
    fn volume_is_range_checked() {
        assert!(Cli::try_parse_from(["winrust-cli", "play", "a.mp3", "--volume", "101"]).is_err());
    }
}
