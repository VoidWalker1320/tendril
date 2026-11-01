//! Command line: `tendril [--tier <tier>] [--accept-eula] [--offline]`.

use crate::settings::TierMode;

pub const USAGE: &str = "\
Usage: tendril [options]

Starts the server in the current folder. The first run creates server.toml,
plugins/, mods/ and logs/, and asks you to accept the Minecraft EULA.

Options:
  --tier <auto|lite|bridge|compat>  Override [tiers] mode from server.toml
  --accept-eula                     Accept the Minecraft EULA without asking
  --offline                         This run only: no account check, no encryption.
                                    For LAN games without internet and for tests;
                                    never use it on a server others can reach.
  -h, --help                        Show this help
  -V, --version                     Show the version";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RunOptions {
    pub tier: Option<TierMode>,
    pub accept_eula: bool,
    pub offline: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Run(RunOptions),
    Help,
    Version,
}

/// Parses arguments after the program name.
///
/// # Errors
/// Returns a message for unknown options or bad values.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut options = RunOptions::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let (flag, inline_value) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
            _ => (arg, None),
        };
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
    fn help_wins() {
        assert_eq!(run(&["--offline", "-h"]), Ok(Command::Help));
    }
}
