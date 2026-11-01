//! Runs `tendril bench` against a `tendril` binary built elsewhere, for machines that can build
//! `tendril-setup` but not Pumpkin (such as a 4 GB phone).
//!
//! `cargo run -p tendril-setup --example bench -- <path/to/tendril> [bench options]`

use std::path::PathBuf;
use std::process::ExitCode;

use tendril_setup::bench::{self, Target, bot::IDS_26_3};
use tendril_setup::cli::{self, Command};
use tendril_setup::say;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(exe) = args.next().map(PathBuf::from) else {
        say("usage: bench <path/to/tendril> [bench options]");
        return ExitCode::from(2);
    };
    let options = match cli::parse(std::iter::once("bench".to_string()).chain(args)) {
        Ok(Command::Bench(options)) => options,
        Ok(_) => {
            say(cli::USAGE);
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            say(&message);
            return ExitCode::from(2);
        }
    };
    let target = Target {
        exe: &exe,
        ids: IDS_26_3,
        version: "external binary",
        commit: None,
    };
    match bench::run(target, &options, &mut std::io::stdout()) {
        Ok(_) => ExitCode::SUCCESS,
        Err(err) => {
            say(&format!("bench failed: {err}"));
            ExitCode::FAILURE
        }
    }
}
