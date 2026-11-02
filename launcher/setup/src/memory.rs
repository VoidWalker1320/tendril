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
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn limit_arenas(max: usize) {
    if std::env::var_os("MALLOC_ARENA_MAX").is_none() {
        let max = libc::c_int::try_from(max).unwrap_or(libc::c_int::MAX);
        // SAFETY: mallopt only tunes the allocator and is called before other threads exist.
        unsafe {
            libc::mallopt(libc::M_ARENA_MAX, max);
        }
    }
}

/// Only glibc has malloc arenas to cap.
#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub const fn limit_arenas(_max: usize) {}

/// Returns free heap pages to the OS.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn release_free_memory() {
    // SAFETY: malloc_trim has no preconditions.
    unsafe {
        libc::malloc_trim(0);
    }
}

/// Returns free heap pages to the OS.
#[cfg(target_os = "android")]
pub fn release_free_memory() {
    android::purge();
}

/// musl returns free memory by itself; macOS and Windows have no portable call for it.
#[cfg(not(any(all(target_os = "linux", target_env = "gnu"), target_os = "android")))]
pub const fn release_free_memory() {}

#[cfg(target_os = "android")]
mod android {
    /// bionic `<malloc.h>`: `M_PURGE` since Android 9, `M_PURGE_ALL` since Android 14.
    const M_PURGE: libc::c_int = -101;
    const M_PURGE_ALL: libc::c_int = -104;

    type Mallopt = unsafe extern "C" fn(libc::c_int, libc::c_int) -> libc::c_int;

    pub fn purge() {
        // mallopt is only exported from Android 8 (API 26) and Tendril supports API 24, so it is
        // looked up at run time instead of linked.
        // SAFETY: dlsym with RTLD_DEFAULT and a NUL-terminated name has no other preconditions.
        let symbol = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"mallopt".as_ptr()) };
        if symbol.is_null() {
            return;
        }
        // SAFETY: bionic declares `int mallopt(int, int)`, which is what `Mallopt` describes.
        let mallopt = unsafe { std::mem::transmute::<*mut libc::c_void, Mallopt>(symbol) };
        // SAFETY: mallopt only asks the allocator to release free pages; bionic returns 0 for
        // options it doesn't know, so older versions fall back to M_PURGE or do nothing.
        unsafe {
            if mallopt(M_PURGE_ALL, 0) == 0 {
                mallopt(M_PURGE, 0);
            }
        }
    }
}
