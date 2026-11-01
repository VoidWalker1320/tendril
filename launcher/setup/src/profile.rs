//! Hardware profiles: desktop, low-RAM and Android (section 11 of the build prompt).

use std::num::NonZero;
use std::path::Path;

use crate::settings::ProfileChoice;

/// Below this much RAM a non-Android host gets the low-RAM profile.
const LOW_RAM_BYTES: u64 = 3 << 30;
/// Phones throttle when every core is busy; two workers keep a server responsive and cool.
const PHONE_WORKER_THREADS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Desktop,
    LowRam,
    Android,
}

impl Kind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::LowRam => "low-ram",
            Self::Android => "android",
        }
    }
}

/// What the host looks like, gathered once at startup.
#[derive(Debug, Clone)]
pub struct Host {
    /// `None` when the OS doesn't report it.
    pub total_ram_bytes: Option<u64>,
    pub cpus: usize,
    /// `Some(how)` when running on Android: natively, in Termux, or in a proot distro on top.
    pub android: Option<&'static str>,
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub kind: Kind,
    pub view_distance: NonZero<u8>,
    pub simulation_distance: NonZero<u8>,
    /// Size of the Tokio runtime and the global Rayon pool.
    pub worker_threads: usize,
    pub reason: String,
}

impl Profile {
    #[must_use]
    pub fn for_kind(kind: Kind, cpus: usize, reason: String) -> Self {
        let (view, simulation, workers) = match kind {
            Kind::Desktop => (8, 6, cpus),
            Kind::LowRam | Kind::Android => (6, 4, PHONE_WORKER_THREADS.min(cpus)),
        };
        Self {
            kind,
            view_distance: NonZero::new(view).unwrap_or(NonZero::<u8>::MIN),
            simulation_distance: NonZero::new(simulation).unwrap_or(NonZero::<u8>::MIN),
            worker_threads: workers.max(1),
            reason,
        }
    }
}

#[must_use]
pub fn detect_host() -> Host {
    let total_ram_bytes = sysinfo::System::new().ok().map(|mut system| {
        system.refresh_memory();
        system.total_memory()
    });
    Host {
        total_ram_bytes,
        cpus: std::thread::available_parallelism().map_or(1, NonZero::get),
        android: detect_android(),
    }
}

fn detect_android() -> Option<&'static str> {
    if cfg!(target_os = "android") {
        return Some(if std::env::var_os("TERMUX_VERSION").is_some() {
            "Termux"
        } else {
            "Android"
        });
    }
    // A glibc build inside proot-distro still runs on a phone: Android's files show through.
    let paths = ["/system/bin/linker64", "/data/data/com.termux"];
    paths
        .iter()
        .any(|p| Path::new(p).exists())
        .then_some("Termux proot")
}

#[must_use]
pub fn choose(choice: ProfileChoice, host: &Host) -> Profile {
    let (kind, reason) = match choice {
        ProfileChoice::Desktop => (Kind::Desktop, "set in server.toml".to_string()),
        ProfileChoice::LowRam => (Kind::LowRam, "set in server.toml".to_string()),
        ProfileChoice::Android => (Kind::Android, "set in server.toml".to_string()),
        ProfileChoice::Auto => match (host.android, host.total_ram_bytes) {
            (Some(how), _) => (Kind::Android, format!("{how} detected")),
            (None, Some(bytes)) if bytes < LOW_RAM_BYTES => (Kind::LowRam, gib(bytes)),
            (None, Some(bytes)) => (Kind::Desktop, gib(bytes)),
            (None, None) => (Kind::Desktop, "RAM unknown".to_string()),
        },
    };
    Profile::for_kind(kind, host.cpus, reason)
}

/// Formats a byte count as `"3.5 GiB RAM"`.
#[must_use]
pub fn gib(bytes: u64) -> String {
    format!("{:.1} GiB RAM", bytes as f64 / f64::from(1u32 << 30))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1 << 30;

    fn host(ram_gib: u64, cpus: usize, android: Option<&'static str>) -> Host {
        Host {
            total_ram_bytes: Some(ram_gib * GIB),
            cpus,
            android,
        }
    }

    #[test]
    fn auto_picks_android_before_ram() {
        let p = choose(ProfileChoice::Auto, &host(12, 8, Some("Termux")));
        assert_eq!(p.kind, Kind::Android);
        assert_eq!(p.worker_threads, 2);
        assert_eq!((p.view_distance.get(), p.simulation_distance.get()), (6, 4));
    }

    #[test]
    fn auto_uses_ram_off_android() {
        assert_eq!(
            choose(ProfileChoice::Auto, &host(2, 4, None)).kind,
            Kind::LowRam
        );
        let desktop = choose(ProfileChoice::Auto, &host(16, 12, None));
        assert_eq!(desktop.kind, Kind::Desktop);
        assert_eq!(desktop.worker_threads, 12);
        assert_eq!(desktop.view_distance.get(), 8);
    }

    #[test]
    fn explicit_choice_wins_over_detection() {
        let p = choose(ProfileChoice::Desktop, &host(4, 8, Some("Termux")));
        assert_eq!(p.kind, Kind::Desktop);
        assert_eq!(p.reason, "set in server.toml");
    }

    #[test]
    fn unknown_ram_off_android_is_desktop() {
        let mut h = host(0, 4, None);
        h.total_ram_bytes = None;
        let p = choose(ProfileChoice::Auto, &h);
        assert_eq!((p.kind, p.reason.as_str()), (Kind::Desktop, "RAM unknown"));
    }

    #[test]
    fn single_core_still_gets_one_worker() {
        assert_eq!(
            choose(ProfileChoice::Android, &host(4, 1, None)).worker_threads,
            1
        );
    }
}
