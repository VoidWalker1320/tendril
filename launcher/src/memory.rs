//! Gives memory the allocator keeps after a burst of work back to the OS while nobody is online.
//!
//! After players leave, most of the heap used for their chunks is free but still held by the
//! allocator. On glibc, capping malloc arenas cut idle RSS by 10-24 MiB on the dev phone (see
//! `docs/m2-report.md`); trimming while empty returns what is left.

use std::time::Duration;

/// How often an empty server hands free memory back.
pub const RELEASE_INTERVAL: Duration = Duration::from_secs(10);

/// Caps glibc's malloc arenas (default: 8 per core), unless `MALLOC_ARENA_MAX` is already set.
/// Call before any other thread starts.
pub fn limit_arenas(max: usize) {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    if std::env::var_os("MALLOC_ARENA_MAX").is_none() {
        let max = libc::c_int::try_from(max).unwrap_or(libc::c_int::MAX);
        // SAFETY: mallopt only tunes the allocator and is called before other threads exist.
        unsafe {
            libc::mallopt(libc::M_ARENA_MAX, max);
        }
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    let _ = max;
}

/// Returns free heap pages to the OS where the allocator supports it (glibc and bionic).
pub fn release_free_memory() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: malloc_trim has no preconditions.
    unsafe {
        libc::malloc_trim(0);
    }
    #[cfg(target_os = "android")]
    android::purge();
}

#[cfg(target_os = "android")]
mod android {
    /// bionic `<malloc.h>`: `M_PURGE` since Android 9, `M_PURGE_ALL` since Android 14.
    const M_PURGE: libc::c_int = -101;
    const M_PURGE_ALL: libc::c_int = -104;

    unsafe extern "C" {
        fn mallopt(param: libc::c_int, value: libc::c_int) -> libc::c_int;
    }

    pub fn purge() {
        // SAFETY: mallopt only asks the allocator to release free pages; bionic returns 0 for
        // options it doesn't know, so older Android versions fall back to M_PURGE or nothing.
        unsafe {
            if mallopt(M_PURGE_ALL, 0) == 0 {
                mallopt(M_PURGE, 0);
            }
        }
    }
}
