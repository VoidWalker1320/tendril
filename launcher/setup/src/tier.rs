//! Picks the lightest tier that can run what is in the server folder (section 5).

use std::path::Path;

use crate::settings::TierMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Lite,
    Bridge,
    Compat,
}

impl Tier {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Lite => "Lite",
            Self::Bridge => "Bridge",
            Self::Compat => "Compat",
        }
    }

    /// Whether this build can run the tier yet.
    #[must_use]
    pub const fn available(self) -> bool {
        matches!(self, Self::Lite)
    }

    /// Milestone that delivers the tier, for messages about missing tiers.
    #[must_use]
    pub const fn milestone(self) -> &'static str {
        match self {
            Self::Lite => "M1",
            Self::Bridge => "M5",
            Self::Compat => "M6",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// The tier the folder's contents need, or the one forced by the operator.
    pub wanted: Tier,
    pub reason: String,
}

/// Counts `.jar` files directly inside `dir`. A missing folder counts as empty.
#[must_use]
pub fn count_jars(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter(|e| {
            Path::new(&e.file_name())
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
        })
        .count()
}

#[must_use]
pub fn select(mode: TierMode, plugin_jars: usize, mod_jars: usize) -> Selection {
    let forced = |tier: Tier| Selection {
        wanted: tier,
        reason: "forced".into(),
    };
    match mode {
        TierMode::Lite => forced(Tier::Lite),
        TierMode::Bridge => forced(Tier::Bridge),
        TierMode::Compat => forced(Tier::Compat),
        TierMode::Auto if mod_jars > 0 => Selection {
            wanted: Tier::Compat,
            reason: format!(
                "{mod_jars} Fabric mod jar{} in mods/",
                if mod_jars == 1 { "" } else { "s" }
            ),
        },
        TierMode::Auto if plugin_jars > 0 => Selection {
            wanted: Tier::Bridge,
            reason: format!(
                "{plugin_jars} Paper plugin jar{} in plugins/",
                if plugin_jars == 1 { "" } else { "s" }
            ),
        },
        TierMode::Auto => Selection {
            wanted: Tier::Lite,
            reason: "no .jar files in plugins/ or mods/".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lightest_tier_that_fits() {
        assert_eq!(select(TierMode::Auto, 0, 0).wanted, Tier::Lite);
        assert_eq!(select(TierMode::Auto, 2, 0).wanted, Tier::Bridge);
        // Fabric mods need the vanilla server, which wins over a Bukkit layer.
        assert_eq!(select(TierMode::Auto, 2, 3).wanted, Tier::Compat);
        assert_eq!(
            select(TierMode::Auto, 0, 1).reason,
            "1 Fabric mod jar in mods/"
        );
    }

    #[test]
    fn forced_tier_ignores_folder() {
        assert_eq!(select(TierMode::Lite, 5, 5).wanted, Tier::Lite);
        assert_eq!(select(TierMode::Compat, 0, 0).wanted, Tier::Compat);
    }

    #[test]
    fn only_jar_files_count() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.jar", "B.JAR", "notes.txt", "plugin.wasm", "jar"] {
            std::fs::write(dir.path().join(name), b"").unwrap();
        }
        std::fs::create_dir(dir.path().join("folder.jar")).unwrap();
        assert_eq!(count_jars(dir.path()), 2);
        assert_eq!(count_jars(&dir.path().join("missing")), 0);
    }
}
