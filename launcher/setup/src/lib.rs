//! Tendril's setup logic: `server.toml`, first run, profiles and tier selection.
//!
//! The `tendril` binary (`launcher/src/main.rs`) uses this to build Pumpkin's configuration and
//! then starts `pumpkin-core`. This crate stays free of `pumpkin-core` and `pumpkin-data` so it
//! builds and tests on a phone.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod bench;
pub mod cli;
pub mod first_run;
pub mod profile;
pub mod report;
pub mod settings;
pub mod tier;

/// Product name. Renaming Tendril is this line plus the `[[bin]]` name in `launcher/Cargo.toml`.
pub const NAME: &str = "Tendril";

/// Shown at startup and in `--version` (section 15 of the build prompt).
pub const DISCLAIMER: &str =
    "Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft.";

/// Writes a line for the operator before the logger exists (usage, EULA prompt, config errors).
#[expect(
    clippy::print_stderr,
    reason = "the logger is configured from server.toml, so earlier messages go to stderr"
)]
pub fn say(message: &str) {
    eprintln!("{message}");
}

#[cfg(test)]
mod upstream_mirror {
    /// FNV-1a of upstream `crates/pumpkin/src/main.rs` at 6458950.
    const UPSTREAM_MAIN_FNV1A: u64 = 0x10d2_e7b1_9e8e_dcd8;

    fn fnv1a(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
        })
    }

    #[test]
    fn upstream_main_unchanged() {
        let upstream = include_bytes!("../../../crates/pumpkin/src/main.rs");
        assert_eq!(
            fnv1a(upstream),
            UPSTREAM_MAIN_FNV1A,
            "crates/pumpkin/src/main.rs changed upstream: port what applies to launcher/src/main.rs \
             and launcher/src/process.rs, then update UPSTREAM_MAIN_FNV1A"
        );
    }
}
