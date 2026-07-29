mod cli;

use std::process::ExitCode;

fn main() -> ExitCode {
    let code = cli::run();
    ExitCode::from(code as u8)
}
