//! The `anyseg` command: segmentation of text from files and the standard input

use std::process::ExitCode;

fn main() -> ExitCode {
    let args = std::env::args_os().skip(1).collect();
    ExitCode::from(anyseg::cli::run(args, env!("CARGO_PKG_VERSION")))
}
