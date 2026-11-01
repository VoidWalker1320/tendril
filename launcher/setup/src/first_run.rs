//! First run in an empty folder: config file, folders and the EULA (section 12).

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;

use crate::settings::{FILE_NAME, TEMPLATE};

pub const EULA_FILE: &str = "eula.txt";
pub const EULA_URL: &str = "https://aka.ms/MinecraftEULA";
/// Created up front so operators can see where things go. The world folder is created by
/// Pumpkin under the name set in `server.toml`.
pub const FOLDERS: [&str; 3] = ["plugins", "mods", "logs"];

#[derive(Debug, thiserror::Error)]
pub enum FirstRunError {
    #[error("cannot prepare the server folder: {0}")]
    Io(#[from] std::io::Error),
    #[error(
        "the Minecraft EULA ({EULA_URL}) has not been accepted; run again with --accept-eula or answer the prompt in a terminal"
    )]
    EulaNotAccepted,
}

/// What `prepare` had to create.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Prepared {
    pub created_config: bool,
    pub accepted_eula_now: bool,
}

/// Creates `server.toml` and the folders if missing, then makes sure the EULA is accepted.
///
/// # Errors
/// Fails on I/O errors, or when the EULA is not accepted and cannot be asked for.
pub fn prepare(dir: &Path, accept_eula: bool) -> Result<Prepared, FirstRunError> {
    let mut prepared = Prepared::default();
    let config = dir.join(FILE_NAME);
    if !config.exists() {
        std::fs::write(&config, TEMPLATE)?;
        prepared.created_config = true;
    }
    for folder in FOLDERS {
        std::fs::create_dir_all(dir.join(folder))?;
    }
    if !eula_accepted(dir)? {
        let stdin = std::io::stdin();
        let yes = accept_eula || (stdin.is_terminal() && ask_eula(&mut stdin.lock())?);
        if !yes {
            return Err(FirstRunError::EulaNotAccepted);
        }
        write_eula(dir)?;
        prepared.accepted_eula_now = true;
    }
    Ok(prepared)
}

/// Reads `eula.txt` in the vanilla format so the Compat tier can reuse the answer.
fn eula_accepted(dir: &Path) -> std::io::Result<bool> {
    match std::fs::read_to_string(dir.join(EULA_FILE)) {
        Ok(text) => Ok(text
            .lines()
            .map(str::trim)
            .any(|l| l.eq_ignore_ascii_case("eula=true"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

fn write_eula(dir: &Path) -> std::io::Result<()> {
    std::fs::write(
        dir.join(EULA_FILE),
        format!(
            "# Accepted the Minecraft EULA ({EULA_URL}) on first run of {}.\neula=true\n",
            crate::NAME
        ),
    )
}

fn ask_eula(input: &mut impl BufRead) -> std::io::Result<bool> {
    let mut stderr = std::io::stderr();
    write!(
        stderr,
        "{} needs you to accept the Minecraft EULA: {EULA_URL}\nDo you accept it? [y/N] ",
        crate::NAME
    )?;
    stderr.flush()?;
    let mut answer = String::new();
    input.read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "Yes" | "YES"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_folder_gets_config_folders_and_eula() {
        let dir = tempfile::tempdir().unwrap();
        let p = prepare(dir.path(), true).unwrap();
        assert_eq!(
            p,
            Prepared {
                created_config: true,
                accepted_eula_now: true
            }
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join(FILE_NAME)).unwrap(),
            TEMPLATE
        );
        for f in FOLDERS {
            assert!(dir.path().join(f).is_dir(), "{f}");
        }
        assert!(eula_accepted(dir.path()).unwrap());
    }

    #[test]
    fn second_run_keeps_edited_config() {
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path(), true).unwrap();
        std::fs::write(dir.path().join(FILE_NAME), "[server]\nmax_players = 3\n").unwrap();
        let p = prepare(dir.path(), false).unwrap();
        assert_eq!(p, Prepared::default());
        assert_eq!(
            std::fs::read_to_string(dir.path().join(FILE_NAME)).unwrap(),
            "[server]\nmax_players = 3\n"
        );
    }

    #[test]
    fn eula_false_is_not_accepted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(EULA_FILE), "eula=false\n").unwrap();
        assert!(!eula_accepted(dir.path()).unwrap());
    }

    #[test]
    fn prompt_needs_an_explicit_yes() {
        for (answer, expected) in [
            ("y\n", true),
            ("yes\n", true),
            ("\n", false),
            ("no\n", false),
        ] {
            assert_eq!(
                ask_eula(&mut answer.as_bytes()).unwrap(),
                expected,
                "{answer:?}"
            );
        }
    }
}
