//! Facts printed once the server is up: join addresses, startup time and memory.

use std::net::{IpAddr, UdpSocket};

/// The address other devices on the LAN use to reach this one.
///
/// Connecting a UDP socket only picks a route; no packet is sent.
#[must_use]
pub fn lan_ipv4() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

/// Resident set size of this process in bytes (`VmRSS` on Linux and Android).
#[must_use]
pub fn rss_bytes() -> Option<u64> {
    let pid = sysinfo::get_current_pid().ok()?;
    let mut system = sysinfo::System::new().ok()?;
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::Some(&[pid]),
        false,
        sysinfo::ProcessRefreshKind::nothing().with_memory(),
    );
    system.process(pid).map(sysinfo::Process::memory)
}

#[must_use]
pub fn mib(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / f64::from(1u32 << 20))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_is_plausible() {
        let rss = rss_bytes().unwrap();
        assert!((1 << 20..1 << 34).contains(&rss), "{rss}");
    }
}
