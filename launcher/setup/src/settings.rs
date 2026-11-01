//! `server.toml`: the one file operators edit, translated into Pumpkin's configuration.

use std::net::SocketAddr;
use std::num::NonZero;
use std::path::Path;

use pumpkin_config::chunk::{AnvilChunkConfig, ChunkConfig};
use pumpkin_config::{LoadConfiguration, PumpkinConfig};
use pumpkin_util::version::JavaMinecraftVersion;
use pumpkin_util::world_seed::Seed;
use serde::Deserialize;

use crate::profile::Profile;

pub const FILE_NAME: &str = "server.toml";

/// Oldest Java release the multi-version layer is meant to accept (section 9).
const OLDEST_JAVA: (u32, u32, u32) = (1, 20, 0);
/// Pumpkin accepts 2..=64; above 32 the memory budget is meaningless.
const MAX_DISTANCE: u8 = 32;
const MIN_DISTANCE: u8 = 2;

/// Written on first run. `Settings::default()` must parse back from it (see tests).
pub const TEMPLATE: &str = r#"# Tendril server configuration. This is the only file you need to edit.
# Restart the server after changing it.

[server]
motd = "A lightweight 26.3 server"
max_players = 20
# true: players need a Microsoft account (Java) or Xbox account (Bedrock). Keep this on for
# anything reachable from the internet.
online_mode = true
# auto | desktop | low-ram | android. Sets default view distances and worker threads.
profile = "auto"
# Anonymous heartbeat to Pumpkin's statistics server every 5 minutes. Off unless you opt in.
telemetry = false

[network]
java_port = 25565
# Bedrock uses NetherNet on this port over both TCP and UDP. Open both in your firewall.
bedrock_port = 19132
bedrock_enabled = true
# Prefix for Bedrock player names so they never collide with Java names.
bedrock_username_prefix = "."
# Oldest Java client to accept. Only 26.3 clients can join until multi-version support lands.
min_java_version = "1.20"

[world]
name = "world"
# Empty = random. Only used when the world is first created.
seed = ""
# Leave these commented to use the profile default (desktop: 8 and 6, low-ram/android: 6 and 4).
# view_distance = 8
# simulation_distance = 6
# anvil | linear | pump. Anvil is the vanilla format and the only one other tiers can open.
format = "anvil"

[tiers]
# auto | lite | bridge | compat. Only lite exists so far.
mode = "auto"
# Empty = auto-detect. Used by the bridge and compat tiers.
java_path = ""
bridge_max_heap = "256M"
compat_max_heap = "2G"

[updates]
# Not implemented yet.
check = true
"#;

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("cannot read {FILE_NAME}: {0}")]
    Read(#[from] std::io::Error),
    #[error("{FILE_NAME} is not valid: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("{FILE_NAME}: {0}")]
    Invalid(String),
}

#[derive(Debug, Deserialize, PartialEq, Eq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub server: Server,
    pub network: Network,
    pub world: World,
    pub tiers: Tiers,
    pub updates: Updates,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Server {
    pub motd: String,
    pub max_players: u32,
    pub online_mode: bool,
    pub profile: ProfileChoice,
    pub telemetry: bool,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            motd: "A lightweight 26.3 server".into(),
            max_players: 20,
            online_mode: true,
            profile: ProfileChoice::Auto,
            telemetry: false,
        }
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Network {
    pub java_port: u16,
    pub bedrock_port: u16,
    pub bedrock_enabled: bool,
    pub bedrock_username_prefix: String,
    pub min_java_version: String,
}

impl Default for Network {
    fn default() -> Self {
        Self {
            java_port: 25565,
            bedrock_port: 19132,
            bedrock_enabled: true,
            bedrock_username_prefix: ".".into(),
            min_java_version: "1.20".into(),
        }
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct World {
    pub name: String,
    pub seed: String,
    pub view_distance: Option<u8>,
    pub simulation_distance: Option<u8>,
    pub format: WorldFormat,
}

impl Default for World {
    fn default() -> Self {
        Self {
            name: "world".into(),
            seed: String::new(),
            view_distance: None,
            simulation_distance: None,
            format: WorldFormat::Anvil,
        }
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Tiers {
    pub mode: TierMode,
    pub java_path: String,
    pub bridge_max_heap: String,
    pub compat_max_heap: String,
}

impl Default for Tiers {
    fn default() -> Self {
        Self {
            mode: TierMode::Auto,
            java_path: String::new(),
            bridge_max_heap: "256M".into(),
            compat_max_heap: "2G".into(),
        }
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Updates {
    pub check: bool,
}

impl Default for Updates {
    fn default() -> Self {
        Self { check: true }
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileChoice {
    #[default]
    Auto,
    Desktop,
    LowRam,
    Android,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WorldFormat {
    #[default]
    Anvil,
    Linear,
    Pump,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TierMode {
    #[default]
    Auto,
    Lite,
    Bridge,
    Compat,
}

impl Settings {
    /// Reads and validates `server.toml` in `dir`.
    ///
    /// # Errors
    /// Fails when the file cannot be read, is not valid TOML, has unknown keys or bad values.
    pub fn load(dir: &Path, current: JavaMinecraftVersion) -> Result<Self, SettingsError> {
        let text = std::fs::read_to_string(dir.join(FILE_NAME))?;
        let settings: Self = toml::from_str(&text)?;
        settings.validate(current)?;
        Ok(settings)
    }

    fn validate(&self, current: JavaMinecraftVersion) -> Result<(), SettingsError> {
        let invalid = |msg: String| Err(SettingsError::Invalid(msg));
        if self.network.java_port == 0 || self.network.bedrock_port == 0 {
            return invalid("ports must be between 1 and 65535".into());
        }
        // NetherNet signaling is TCP, so the two ports would collide.
        if self.network.bedrock_enabled && self.network.java_port == self.network.bedrock_port {
            return invalid(format!(
                "java_port and bedrock_port are both {}; Bedrock also uses TCP on its port",
                self.network.java_port
            ));
        }
        if self.server.max_players == 0 {
            return invalid("max_players must be at least 1".into());
        }
        for (key, value) in [
            ("view_distance", self.world.view_distance),
            ("simulation_distance", self.world.simulation_distance),
        ] {
            if let Some(v) = value
                && !(MIN_DISTANCE..=MAX_DISTANCE).contains(&v)
            {
                return invalid(format!(
                    "{key} = {v} is outside {MIN_DISTANCE}..={MAX_DISTANCE}"
                ));
            }
        }
        let name = &self.world.name;
        if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
            return invalid(format!("world name {name:?} must be a plain folder name"));
        }
        for (key, value) in [
            ("bridge_max_heap", &self.tiers.bridge_max_heap),
            ("compat_max_heap", &self.tiers.compat_max_heap),
        ] {
            if parse_heap(value).is_none() {
                return invalid(format!(
                    "{key} = {value:?} must look like \"256M\" or \"2G\""
                ));
            }
        }
        parse_min_java_version(&self.network.min_java_version, current)
            .map_err(SettingsError::Invalid)?;
        Ok(())
    }

    /// Builds Pumpkin's configuration. Distances not set in `server.toml` come from the profile.
    #[must_use]
    pub fn to_pumpkin(&self, profile: &Profile) -> PumpkinConfig {
        let mut config = PumpkinConfig::default();
        let view = distance(self.world.view_distance, profile.view_distance);
        let simulation = distance(self.world.simulation_distance, profile.simulation_distance);

        config.basic.seed = Seed::from(self.world.seed.as_str());
        config.basic.default_level_name.clone_from(&self.world.name);

        let java = &mut config.advanced.networking.java;
        java.address = SocketAddr::from(([0, 0, 0, 0], self.network.java_port));
        java.online_mode = self.server.online_mode;
        // Vanilla only encrypts in online mode.
        java.encryption = self.server.online_mode;
        java.max_players = self.server.max_players;
        java.view_distance = view;
        java.simulation_distance = simulation;
        java.motd.clone_from(&self.server.motd);

        let bedrock = &mut config.advanced.networking.bedrock;
        bedrock.enabled = self.network.bedrock_enabled;
        bedrock.online_mode = self.server.online_mode;
        bedrock.max_players = self.server.max_players;
        bedrock.view_distance = view;
        bedrock.simulation_distance = simulation;
        bedrock.motd.clone_from(&self.server.motd);
        bedrock
            .username_prefix
            .clone_from(&self.network.bedrock_username_prefix);
        bedrock.replace_username_spaces = true;
        bedrock.nethernet.address = SocketAddr::from(([0, 0, 0, 0], self.network.bedrock_port));

        config.advanced.world.chunk = match self.world.format {
            WorldFormat::Anvil => ChunkConfig::Anvil(AnvilChunkConfig::default()),
            WorldFormat::Linear => ChunkConfig::Linear,
            WorldFormat::Pump => ChunkConfig::Pump,
        };
        config.telemetry.enabled = self.server.telemetry;

        // Our ranges are stricter than Pumpkin's, so this only guards against upstream changes.
        config.validate();
        config
    }
}

fn distance(configured: Option<u8>, profile_default: NonZero<u8>) -> NonZero<u8> {
    configured.and_then(NonZero::new).unwrap_or(profile_default)
}

/// Parses a JVM heap size like `256M` or `2G` into bytes.
#[must_use]
pub fn parse_heap(value: &str) -> Option<u64> {
    let value = value.trim();
    let (digits, unit) = value.split_at(value.len().checked_sub(1)?);
    let shift = match unit {
        "M" | "m" => 20,
        "G" | "g" => 30,
        _ => return None,
    };
    let n: u64 = digits.parse().ok().filter(|n| *n > 0)?;
    n.checked_mul(1 << shift)
}

/// Resolves a release name to the oldest protocol it implies, using Pumpkin's version table.
///
/// Pumpkin keeps one entry per protocol, named after the first release that used it, so
/// `1.21.10` resolves to the `1.21.9` entry (protocol 773).
///
/// # Errors
/// Fails for names that are not versions or fall outside 1.20 to the current release.
pub fn parse_min_java_version(
    name: &str,
    current: JavaMinecraftVersion,
) -> Result<JavaMinecraftVersion, String> {
    let wanted = parse_release(name).ok_or_else(|| {
        format!("min_java_version {name:?} is not a version like \"1.20\" or \"26.1\"")
    })?;
    let newest = parse_release(&current.to_string()).unwrap_or(wanted);
    if wanted < OLDEST_JAVA || wanted > newest {
        return Err(format!(
            "min_java_version {name:?} must be between 1.20 and {current}"
        ));
    }
    let mut best: Option<((u32, u32, u32), JavaMinecraftVersion)> = None;
    for protocol in 0..=current.protocol_version().unsigned_abs() {
        let version = JavaMinecraftVersion::from_protocol(protocol);
        let Some(release) = parse_release(&version.to_string()) else {
            continue;
        };
        if release <= wanted && best.is_none_or(|(b, _)| release > b) {
            best = Some((release, version));
        }
    }
    best.map(|(_, v)| v)
        .ok_or_else(|| format!("min_java_version {name:?} has no known protocol"))
}

fn parse_release(name: &str) -> Option<(u32, u32, u32)> {
    let mut parts = name.trim().split('.').map(str::parse::<u32>);
    let major = parts.next()?.ok()?;
    let minor = parts.next()?.ok()?;
    let patch = match parts.next() {
        Some(p) => p.ok()?,
        None => 0,
    };
    parts.next().is_none().then_some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Kind, Profile};

    const CURRENT: JavaMinecraftVersion = JavaMinecraftVersion::V_26_3;

    fn desktop() -> Profile {
        Profile::for_kind(Kind::Desktop, 8, "test".into())
    }

    #[test]
    fn template_parses_to_defaults() {
        let parsed: Settings = toml::from_str(TEMPLATE).unwrap();
        assert_eq!(parsed, Settings::default());
        parsed.validate(CURRENT).unwrap();
    }

    #[test]
    fn unknown_key_is_rejected() {
        let err = toml::from_str::<Settings>("[world]\nview_radius = 8\n").unwrap_err();
        assert!(err.to_string().contains("view_radius"), "{err}");
    }

    #[test]
    fn same_java_and_bedrock_port_is_rejected() {
        let s: Settings = toml::from_str("[network]\njava_port = 19132\n").unwrap();
        assert!(s.validate(CURRENT).is_err());
        let s: Settings =
            toml::from_str("[network]\njava_port = 19132\nbedrock_enabled = false\n").unwrap();
        assert!(s.validate(CURRENT).is_ok());
    }

    #[test]
    fn distance_bounds() {
        for (text, ok) in [("1", false), ("2", true), ("32", true), ("33", false)] {
            let s: Settings =
                toml::from_str(&format!("[world]\nview_distance = {text}\n")).unwrap();
            assert_eq!(s.validate(CURRENT).is_ok(), ok, "view_distance = {text}");
        }
    }

    #[test]
    fn world_name_must_be_a_folder() {
        for bad in ["", "..", "a/b", "a\\b"] {
            let mut s = Settings::default();
            s.world.name = bad.into();
            assert!(s.validate(CURRENT).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn heap_sizes() {
        assert_eq!(parse_heap("256M"), Some(256 << 20));
        assert_eq!(parse_heap("2g"), Some(2 << 30));
        for bad in ["", "M", "0G", "2T", "2.5G", "-1G"] {
            assert_eq!(parse_heap(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn min_java_version_maps_to_first_release_of_its_protocol() {
        let protocol = |s: &str| {
            parse_min_java_version(s, CURRENT)
                .unwrap()
                .protocol_version()
        };
        assert_eq!(protocol("1.20"), 763);
        assert_eq!(protocol("1.20.1"), 763);
        assert_eq!(protocol("1.20.2"), 764);
        assert_eq!(protocol("1.21.10"), 773);
        assert_eq!(protocol("26.1.2"), 775);
        assert_eq!(protocol("26.3"), 777);
        for bad in ["1.19.4", "27.0", "latest", "1", "1.20.1.1"] {
            assert!(parse_min_java_version(bad, CURRENT).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn translation_sets_pumpkin_fields() {
        let mut s = Settings::default();
        s.network.java_port = 25570;
        s.network.bedrock_port = 19140;
        s.world.format = WorldFormat::Pump;
        let c = s.to_pumpkin(&desktop());
        let java = &c.advanced.networking.java;
        assert_eq!(java.address.port(), 25570);
        assert!(java.online_mode && java.encryption);
        assert_eq!(java.max_players, 20);
        assert_eq!(
            c.advanced.networking.bedrock.nethernet.address.port(),
            19140
        );
        assert_eq!(c.advanced.networking.bedrock.username_prefix, ".");
        assert!(matches!(c.advanced.world.chunk, ChunkConfig::Pump));
        assert!(!c.telemetry.enabled);
    }

    #[test]
    fn offline_mode_disables_encryption_like_vanilla() {
        let mut s = Settings::default();
        s.server.online_mode = false;
        let c = s.to_pumpkin(&desktop());
        assert!(!c.advanced.networking.java.online_mode);
        assert!(!c.advanced.networking.java.encryption);
        assert!(!c.advanced.networking.bedrock.online_mode);
    }

    #[test]
    fn explicit_distances_override_the_profile() {
        let phone = Profile::for_kind(Kind::Android, 8, "test".into());
        let mut s = Settings::default();
        let c = s.to_pumpkin(&phone);
        assert_eq!(c.advanced.networking.java.view_distance.get(), 6);
        assert_eq!(c.advanced.networking.java.simulation_distance.get(), 4);
        s.world.view_distance = Some(10);
        let c = s.to_pumpkin(&phone);
        assert_eq!(c.advanced.networking.java.view_distance.get(), 10);
        assert_eq!(c.advanced.networking.bedrock.view_distance.get(), 10);
        assert_eq!(c.advanced.networking.java.simulation_distance.get(), 4);
    }
}
