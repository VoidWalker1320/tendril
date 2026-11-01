//! Command line: `tendril [--tier <tier>] [--accept-eula] [--offline]` and `tendril bench`.

use std::path::PathBuf;

use crate::bench;
use crate::settings::{TierMode, WorldFormat};

pub const USAGE: &str = "\
Usage: tendril [options]
       tendril bench [bench options]

Starts the server in the current folder. The first run creates server.toml,
plugins/, mods/ and logs/, and asks you to accept the Minecraft EULA.

Options:
  --tier <auto|lite|bridge|compat>  Override [tiers] mode from server.toml
  --accept-eula                     Accept the Minecraft EULA without asking
  --offline                         This run only: no account check, no encryption.
                                    For LAN games without internet and for tests;
                                    never use it on a server others can reach.
  -h, --help                        Show this help
  -V, --version                     Show the version

Bench runs this server in a scratch folder with bots and prints startup time,
idle memory and CPU, memory per player and tick times (3 runs, about 8 min):
  --quick                           One short run, for a quick check
  --runs <n>                        Number of measured runs (default 3)
  --format <anvil|linear|pump>      World format to test (default anvil)
  --keep                            Keep the scratch folder afterwards
  --json <path>                     Where to write the full results";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RunOptions {
    pub tier: Option<TierMode>,
    pub accept_eula: bool,
    pub offline: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Run(RunOptions),
    Bench(bench::Options),
    Help,
    Version,
}

/// Parses arguments after the program name.
///
/// # Errors
/// Returns a message for unknown options or bad values.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut args = args.into_iter().peekable();
    if args.peek().is_some_and(|a| a == "bench") {
        args.next();
        return parse_bench(args);
    }
    let mut options = RunOptions::default();
    while let Some(arg) = args.next() {
        let (flag, inline_value) = split_flag(arg);
        match flag.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "--accept-eula" => options.accept_eula = true,
            "--offline" => options.offline = true,
            "--tier" => {
                let value = inline_value
                    .or_else(|| args.next())
                    .ok_or("--tier needs a value: auto, lite, bridge or compat")?;
                options.tier = Some(match value.as_str() {
                    "auto" => TierMode::Auto,
                    "lite" => TierMode::Lite,
                    "bridge" => TierMode::Bridge,
                    "compat" => TierMode::Compat,
                    other => {
                        return Err(format!(
                            "unknown tier {other:?}; use auto, lite, bridge or compat"
                        ));
                    }
                });
            }
            other => return Err(format!("unknown option {other:?}; see --help")),
        }
    }
    Ok(Command::Run(options))
}

fn parse_bench(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut options = bench::Options::full();
    while let Some(arg) = args.next() {
        let (flag, inline_value) = split_flag(arg);
        let mut value = |what: &str| {
            inline_value
                .clone()
                .or_else(|| args.next())
                .ok_or_else(|| format!("{flag} needs {what}"))
        };
        match flag.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--quick" => {
                options = bench::Options {
                    format: options.format,
                    keep: options.keep,
                    json: options.json,
                    ..bench::Options::quick()
                };
            }
            "--runs" => {
                options.runs = value("a number")?
                    .parse()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or("--runs needs a number above 0")?;
            }
            "--format" => {
                options.format = match value("anvil, linear or pump")?.as_str() {
                    "anvil" => WorldFormat::Anvil,
                    "linear" => WorldFormat::Linear,
                    "pump" => WorldFormat::Pump,
                    other => {
                        return Err(format!(
                            "unknown format {other:?}; use anvil, linear or pump"
                        ));
                    }
                };
            }
            "--keep" => options.keep = true,
            "--json" => options.json = Some(PathBuf::from(value("a path")?)),
            other => return Err(format!("unknown bench option {other:?}; see --help")),
        }
    }
    Ok(Command::Bench(options))
}

/// Splits `--flag=value` into its parts; other arguments come back unchanged.
fn split_flag(arg: String) -> (String, Option<String>) {
    match arg.split_once('=') {
        Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
        _ => (arg, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(|s| (*s).to_string()))
    }

    #[test]
    fn no_arguments_starts_the_server() {
        assert_eq!(run(&[]), Ok(Command::Run(RunOptions::default())));
    }

    #[test]
    fn tier_accepts_both_spellings() {
        let expected = Command::Run(RunOptions {
            tier: Some(TierMode::Compat),
            accept_eula: true,
            ..RunOptions::default()
        });
        assert_eq!(run(&["--tier", "compat", "--accept-eula"]), Ok(expected));
        let Ok(Command::Run(o)) = run(&["--tier=lite", "--offline"]) else {
            panic!("expected run");
        };
        assert_eq!((o.tier, o.offline), (Some(TierMode::Lite), true));
    }

    #[test]
    fn bad_input_is_an_error() {
        assert!(run(&["--tier"]).is_err());
        assert!(run(&["--tier", "paper"]).is_err());
        assert!(run(&["--port", "1"]).is_err());
        assert!(run(&["start"]).is_err());
    }

    #[test]
    fn bench_options() {
        assert_eq!(run(&["bench"]), Ok(Command::Bench(bench::Options::full())));
        let Ok(Command::Bench(o)) = run(&["bench", "--runs", "5", "--format=pump", "--keep"])
        else {
            panic!("expected bench");
        };
        assert_eq!((o.runs, o.format, o.keep), (5, WorldFormat::Pump, true));
        let Ok(Command::Bench(o)) = run(&["bench", "--format", "linear", "--quick"]) else {
            panic!("expected bench");
        };
        assert_eq!((o.runs, o.format), (1, WorldFormat::Linear));
        assert!(run(&["bench", "--runs", "0"]).is_err());
        assert!(run(&["bench", "--offline"]).is_err());
        assert!(run(&["--offline", "bench"]).is_err());
    }

    #[test]
    fn help_wins() {
        assert_eq!(run(&["--offline", "-h"]), Ok(Command::Help));
    }
}
