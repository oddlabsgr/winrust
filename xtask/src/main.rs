//! Developer tasks: `cargo xtask <task>`. Dependency-free on purpose so it builds instantly.

use std::process::ExitCode;

const HELP: &str = "\
Usage: cargo xtask <task>

Tasks:
  help    Show this message
";

fn main() -> ExitCode {
    let task = std::env::args().nth(1);
    match task.as_deref() {
        None | Some("help") | Some("--help") | Some("-h") => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown task: {other}\n\n{HELP}");
            ExitCode::FAILURE
        }
    }
}
