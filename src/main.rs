mod cli;

use std::process::ExitCode;

fn main() -> ExitCode {
    recall::telemetry::install_crash_hook();
    let code = cli::run();
    ExitCode::from(code as u8)
}
