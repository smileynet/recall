mod cli;
mod embed;
mod ingest;
mod scan;
mod search;
mod store;

use std::process::ExitCode;

fn main() -> ExitCode {
    let code = cli::run();
    ExitCode::from(code as u8)
}
