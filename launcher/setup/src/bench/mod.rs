//! `tendril bench`: startup, idle memory and CPU, memory per player and tick times (section 14).
//!
//! The server under test is this same binary, started as a child process in a scratch folder
//! with a fixed seed, offline mode and free ports. A warm-up run creates the world; the measured
//! runs reuse it, as the budgets are defined on an existing world.

pub mod bot;
pub mod server;

use std::io::{self, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::profile::{self, Kind};
use crate::settings::WorldFormat;
use bot::{Bot, PacketIds};
use server::{ServerProcess, Usage};

/// Section 4: idle RSS of the Lite tier, in bytes ("100 MB").
pub const IDLE_RSS_BUDGET: u64 = 100_000_000;
/// Section 4: startup budget on a desktop and on a phone.
pub const STARTUP_BUDGET_DESKTOP: Duration = Duration::from_secs(1);
pub const STARTUP_BUDGET_PHONE: Duration = Duration::from_secs(3);

const BOOT_TIMEOUT: Duration = Duration::from_secs(180);
const JOIN_TIMEOUT: Duration = Duration::from_secs(120);
const STOP_TIMEOUT: Duration = Duration::from_secs(120);
/// Delay between bot logins in a load phase, so logins don't all land in one tick.
const BOT_STAGGER: Duration = Duration::from_millis(150);
/// Time allowed for every bot of a phase to finish joining before the clock starts.
const JOIN_SLACK: Duration = Duration::from_secs(20);
const SETTLE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub runs: usize,
    pub idle: Duration,
    /// Window after `idle` for steady-state CPU; covers the empty-server pause at 60 s.
    pub steady: Duration,
    pub load: Duration,
    pub bot_counts: Vec<usize>,
    pub format: WorldFormat,
    pub keep: bool,
    pub json: Option<PathBuf>,
}

impl Options {
    /// Section 14's method: three runs, 60 s idle, 1, 10 and 20 bots.
    #[must_use]
    pub fn full() -> Self {
        Self {
            runs: 3,
            idle: Duration::from_secs(60),
            steady: Duration::from_secs(15),
            load: Duration::from_secs(30),
            bot_counts: vec![1, 10, 20],
            format: WorldFormat::Anvil,
            keep: false,
            json: None,
        }
    }

    /// One short run for smoke tests; its numbers don't meet section 14's method.
    #[must_use]
    pub fn quick() -> Self {
        Self {
            runs: 1,
            idle: Duration::from_secs(10),
            steady: Duration::from_secs(5),
            load: Duration::from_secs(10),
            bot_counts: vec![1, 5],
            ..Self::full()
        }
    }
}

/// What the benchmark needs from the `tendril` binary.
#[derive(Debug, Clone, Copy)]
pub struct Target<'a> {
    pub exe: &'a Path,
    pub ids: PacketIds,
    pub version: &'a str,
    pub commit: Option<&'a str>,
}

#[derive(Debug, thiserror::Error)]
pub enum BenchError {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("bot: {0}")]
    Bot(#[from] bot::BotError),
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Mspt {
    pub avg: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoadSample {
    pub bots: usize,
    pub joined: usize,
    pub rss_bytes: u64,
    pub swap_bytes: Option<u64>,
    pub mspt: Option<Mspt>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Run {
    pub port_open_ms: f64,
    pub spawned_ms: f64,
    pub idle_rss_bytes: u64,
    pub idle_swap_bytes: Option<u64>,
    /// CPU while idle, from the bot leaving until `idle` has passed.
    pub idle_cpu_percent: f64,
    /// CPU in the `steady` window after that, when an empty server has paused.
    pub steady_idle_cpu_percent: f64,
    pub idle_mspt: Option<Mspt>,
    pub load: Vec<LoadSample>,
    pub shutdown_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Machine {
    pub os: String,
    pub arch: &'static str,
    pub kernel: Option<String>,
    pub cpus: usize,
    pub ram_bytes: Option<u64>,
    pub profile: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    pub version: String,
    pub binary_bytes: u64,
    pub commit: Option<String>,
    pub unix_time: u64,
    pub machine: Machine,
    pub format: &'static str,
    pub idle_seconds: u64,
    pub load_seconds: u64,
    pub fresh_world_spawned_ms: f64,
    pub runs: Vec<Run>,
}

/// Runs the benchmark, prints a table to `out` and returns the full report.
///
/// # Errors
/// Fails if the server cannot be started, a bot cannot join, or the scratch folder fails.
pub fn run(
    target: Target<'_>,
    options: &Options,
    out: &mut impl Write,
) -> Result<Report, BenchError> {
    let host = profile::detect_host();
    let profile = profile::choose(crate::settings::ProfileChoice::Auto, &host);
    let dir = scratch_dir()?;
    let java = free_port()?;
    let bedrock = free_port()?;
    write_config(&dir, java, bedrock, options.format)?;
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, java));

    writeln!(out, "{} bench in {}", crate::NAME, dir.display())?;
    writeln!(out, "warm-up: creating the world")?;
    let fresh = warm_up(target, &dir, addr)?;
    let mut runs = Vec::with_capacity(options.runs);
    for i in 1..=options.runs {
        writeln!(out, "run {i}/{}", options.runs)?;
        runs.push(measure(target, &dir, addr, options)?);
    }
    if !options.keep {
        let _ = std::fs::remove_dir_all(&dir);
    }

    let report = Report {
        version: target.version.to_string(),
        binary_bytes: std::fs::metadata(target.exe).map_or(0, |m| m.len()),
        commit: target.commit.map(str::to_string),
        unix_time: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        machine: Machine {
            os: sysinfo::System::long_os_version()
                .unwrap_or_else(|_| std::env::consts::OS.to_string()),
            arch: std::env::consts::ARCH,
            kernel: sysinfo::System::kernel_version().ok(),
            cpus: host.cpus,
            ram_bytes: host.total_ram_bytes,
            profile: profile.kind.name(),
        },
        format: format_name(options.format),
        idle_seconds: options.idle.as_secs(),
        load_seconds: options.load.as_secs(),
        fresh_world_spawned_ms: fresh,
        runs,
    };
    print_table(&report, profile.kind, out)?;
    let json = options
        .json
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("tendril-bench-{}.json", report.unix_time)));
    let text =
        serde_json::to_string_pretty(&report).map_err(|e| BenchError::Other(e.to_string()))?;
    std::fs::write(&json, text)?;
    writeln!(out, "  full results: {}", json.display())?;
    Ok(report)
}

fn warm_up(target: Target<'_>, dir: &Path, addr: SocketAddr) -> Result<f64, BenchError> {
    let mut server = ServerProcess::spawn(target.exe, dir)?;
    server.wait_port(addr, BOOT_TIMEOUT)?;
    let spawned = join_one(&server, addr, target.ids, "warmup")?.1;
    server.stop(STOP_TIMEOUT)?;
    Ok(spawned)
}

fn measure(
    target: Target<'_>,
    dir: &Path,
    addr: SocketAddr,
    options: &Options,
) -> Result<Run, BenchError> {
    let mut server = ServerProcess::spawn(target.exe, dir)?;
    let port_open = server.wait_port(addr, BOOT_TIMEOUT)?;
    let (bot, spawned_ms) = join_one(&server, addr, target.ids, "bench")?;
    drop(bot);
    std::thread::sleep(SETTLE);

    let before = usage(&server)?;
    std::thread::sleep(options.idle);
    let after = usage(&server)?;
    // Tick times stop updating once the server pauses, so this covers the last ticks before it.
    let idle_mspt = tick_query(&mut server)?;
    let steady_start = usage(&server)?;
    std::thread::sleep(options.steady);
    let steady_end = usage(&server)?;

    let mut load = Vec::with_capacity(options.bot_counts.len());
    for &bots in &options.bot_counts {
        load.push(load_phase(
            &mut server,
            addr,
            target.ids,
            bots,
            options.load,
        )?);
        std::thread::sleep(SETTLE);
    }
    let (_, shutdown) = server.stop(STOP_TIMEOUT)?;
    Ok(Run {
        port_open_ms: port_open.as_secs_f64() * 1000.0,
        spawned_ms,
        idle_rss_bytes: after.rss_bytes,
        idle_swap_bytes: after.swap_bytes,
        idle_cpu_percent: cpu_percent(before, after, options.idle),
        steady_idle_cpu_percent: cpu_percent(steady_start, steady_end, options.steady),
        idle_mspt,
        load,
        shutdown_ms: shutdown.as_secs_f64() * 1000.0,
    })
}

/// Joins one bot, retrying while the server still refuses connections. Returns the bot and the
/// time from process start until it spawned.
fn join_one(
    server: &ServerProcess,
    addr: SocketAddr,
    ids: PacketIds,
    name: &str,
) -> Result<(Bot, f64), BenchError> {
    let deadline = Instant::now() + JOIN_TIMEOUT;
    loop {
        let attempt = server.started.elapsed();
        match Bot::join(addr, name, ids, JOIN_TIMEOUT) {
            Ok((bot, timings)) => {
                return Ok((bot, attempt.as_secs_f64() * 1000.0 + timings.spawned_ms));
            }
            Err(bot::BotError::Io(e)) if Instant::now() < deadline => {
                if e.kind() != io::ErrorKind::ConnectionRefused {
                    return Err(bot::BotError::Io(e).into());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

fn load_phase(
    server: &mut ServerProcess,
    addr: SocketAddr,
    ids: PacketIds,
    bots: usize,
    hold: Duration,
) -> Result<LoadSample, BenchError> {
    let stagger = BOT_STAGGER * u32::try_from(bots).unwrap_or(u32::MAX);
    let end = Instant::now() + stagger + JOIN_SLACK + hold;
    let joined = AtomicUsize::new(0);
    let (usage, mspt) = std::thread::scope(|scope| {
        for i in 0..bots {
            let joined = &joined;
            scope.spawn(move || {
                std::thread::sleep(BOT_STAGGER * u32::try_from(i).unwrap_or(u32::MAX));
                if let Ok((mut bot, _)) = Bot::join(addr, &format!("bot{i}"), ids, JOIN_SLACK) {
                    joined.fetch_add(1, Ordering::Relaxed);
                    let _ = bot.pump_until(end);
                }
            });
        }
        // Sample just before the bots leave, after `hold` with everyone online.
        std::thread::sleep(
            end.saturating_duration_since(Instant::now())
                .saturating_sub(SETTLE),
        );
        (usage(server), tick_query(server))
    });
    let usage = usage?;
    Ok(LoadSample {
        bots,
        joined: joined.load(Ordering::Relaxed),
        rss_bytes: usage.rss_bytes,
        swap_bytes: usage.swap_bytes,
        mspt: mspt?,
    })
}

fn cpu_percent(from: Usage, to: Usage, over: Duration) -> f64 {
    to.cpu_ms.saturating_sub(from.cpu_ms) as f64 / (over.as_secs_f64() * 1000.0) * 100.0
}

fn usage(server: &ServerProcess) -> Result<Usage, BenchError> {
    server
        .usage()
        .ok_or_else(|| BenchError::Other("cannot read the server's memory and CPU".into()))
}

/// Asks the server for its tick times with `/tick query`.
fn tick_query(server: &mut ServerProcess) -> Result<Option<Mspt>, BenchError> {
    let from = server.line_count();
    server.command("tick query")?;
    let percentiles = server.wait_for("Percentiles: P50:", from, Duration::from_secs(10))?;
    let average = server.wait_for("Average time per tick:", from, Duration::from_secs(1))?;
    Ok(parse_mspt(&average, &percentiles))
}

/// Parses `Average time per tick: 6.05ms ...` and `Percentiles: P50: 4.96ms P95: ... P99: ...`.
#[must_use]
pub fn parse_mspt(average: &str, percentiles: &str) -> Option<Mspt> {
    Some(Mspt {
        avg: number_after(average, "Average time per tick: ")?,
        p50: number_after(percentiles, "P50: ")?,
        p95: number_after(percentiles, "P95: ")?,
        p99: number_after(percentiles, "P99: ")?,
    })
}

fn number_after(line: &str, key: &str) -> Option<f64> {
    let rest = &line[line.find(key)? + key.len()..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn scratch_dir() -> io::Result<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let dir = std::env::temp_dir().join(format!("tendril-bench-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// A port that is free for both TCP and UDP right now (Bedrock uses both).
fn free_port() -> io::Result<u16> {
    for _ in 0..20 {
        let port = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))?
            .local_addr()?
            .port();
        if UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).is_ok() {
            return Ok(port);
        }
    }
    Err(io::Error::other("no free port"))
}

fn write_config(dir: &Path, java: u16, bedrock: u16, format: WorldFormat) -> io::Result<()> {
    std::fs::write(
        dir.join(crate::settings::FILE_NAME),
        format!(
            "[server]\nmotd = \"Tendril bench\"\n\n[network]\njava_port = {java}\nbedrock_port = {bedrock}\n\n[world]\nseed = \"tendril-bench\"\nformat = \"{}\"\n",
            format_name(format)
        ),
    )
}

const fn format_name(format: WorldFormat) -> &'static str {
    match format {
        WorldFormat::Anvil => "anvil",
        WorldFormat::Linear => "linear",
        WorldFormat::Pump => "pump",
    }
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    Some(if values.len().is_multiple_of(2) {
        f64::midpoint(values[mid - 1], values[mid])
    } else {
        values[mid]
    })
}

fn mib(bytes: f64) -> f64 {
    bytes / f64::from(1u32 << 20)
}

fn print_table(report: &Report, kind: Kind, out: &mut impl Write) -> io::Result<()> {
    let runs = &report.runs;
    let col = |f: &dyn Fn(&Run) -> Option<f64>| median(runs.iter().filter_map(f).collect());
    let m = &report.machine;
    writeln!(out)?;
    writeln!(
        out,
        "{} {} ({:.1} MB) | {} {} | {} CPUs, {} | profile {} | format {} | median of {} run{}",
        crate::NAME,
        report.version,
        report.binary_bytes as f64 / 1e6,
        m.os,
        m.arch,
        m.cpus,
        m.ram_bytes
            .map_or_else(|| "RAM unknown".into(), profile::gib),
        m.profile,
        report.format,
        runs.len(),
        if runs.len() == 1 { "" } else { "s" },
    )?;
    let show = |v: Option<f64>, unit: &str| {
        v.map_or_else(|| "n/a".to_string(), |v| format!("{v:.1} {unit}"))
    };
    let spawned = col(&|r| Some(r.spawned_ms));
    let idle_rss = col(&|r| Some(r.idle_rss_bytes as f64));
    writeln!(out, "  start to bot spawned       {}", show(spawned, "ms"))?;
    writeln!(
        out,
        "  fresh world, bot spawned   {}",
        show(Some(report.fresh_world_spawned_ms), "ms")
    )?;
    writeln!(
        out,
        "  idle RSS after {} s         {} (swap {})",
        report.idle_seconds,
        show(idle_rss.map(mib), "MiB"),
        show(col(&|r| r.idle_swap_bytes.map(|b| mib(b as f64))), "MiB"),
    )?;
    writeln!(
        out,
        "  idle CPU first {} s, then   {}, {}",
        report.idle_seconds,
        show(col(&|r| Some(r.idle_cpu_percent)), "%"),
        show(col(&|r| Some(r.steady_idle_cpu_percent)), "% of a core")
    )?;
    writeln!(
        out,
        "  idle MSPT p50/p95/p99      {} / {} / {}",
        show(col(&|r| r.idle_mspt.map(|m| m.p50)), "ms"),
        show(col(&|r| r.idle_mspt.map(|m| m.p95)), "ms"),
        show(col(&|r| r.idle_mspt.map(|m| m.p99)), "ms"),
    )?;
    let counts: Vec<usize> = runs
        .first()
        .map(|r| r.load.iter().map(|l| l.bots).collect())
        .unwrap_or_default();
    for (i, bots) in counts.iter().enumerate() {
        let at = |f: &dyn Fn(&LoadSample) -> Option<f64>| col(&|r: &Run| r.load.get(i).and_then(f));
        writeln!(
            out,
            "  {bots:>2} bots: RSS {} (swap {}), MSPT p50/p95/p99 {} / {} / {}, joined {}",
            show(at(&|l| Some(mib(l.rss_bytes as f64))), "MiB"),
            show(at(&|l| l.swap_bytes.map(|b| mib(b as f64))), "MiB"),
            show(at(&|l| l.mspt.map(|m| m.p50)), "ms"),
            show(at(&|l| l.mspt.map(|m| m.p95)), "ms"),
            show(at(&|l| l.mspt.map(|m| m.p99)), "ms"),
            at(&|l| Some(l.joined as f64)).map_or_else(|| "n/a".to_string(), |v| format!("{v:.0}")),
        )?;
    }
    let budget = if kind == Kind::Android {
        STARTUP_BUDGET_PHONE
    } else {
        STARTUP_BUDGET_DESKTOP
    };
    let verdict = |ok: bool| if ok { "PASS" } else { "FAIL" };
    if let Some(rss) = idle_rss {
        writeln!(
            out,
            "  budget idle RSS <= 100 MB: {}",
            verdict(rss <= IDLE_RSS_BUDGET as f64)
        )?;
    }
    if let Some(ms) = spawned {
        writeln!(
            out,
            "  budget startup <= {} s ({} profile, Java bot only): {}",
            budget.as_secs(),
            report.machine.profile,
            verdict(ms <= budget.as_secs_f64() * 1000.0)
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_query_lines_parse() {
        let m = parse_mspt(
            "Average time per tick: 6.05ms (Target: 50.00ms)",
            "Percentiles: P50: 4.96ms P95: 13.89ms P99: 17.20ms. Sample: 100",
        )
        .unwrap();
        assert_eq!((m.avg, m.p50, m.p95, m.p99), (6.05, 4.96, 13.89, 17.2));
        assert!(parse_mspt("nothing", "here").is_none());
    }

    #[test]
    fn median_of_odd_and_even() {
        assert_eq!(median(vec![3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(vec![4.0, 1.0, 2.0, 3.0]), Some(2.5));
        assert_eq!(median(Vec::new()), None);
    }

    #[test]
    fn bench_config_is_valid_settings() {
        let dir = tempfile::tempdir().unwrap();
        write_config(dir.path(), 25601, 19201, WorldFormat::Pump).unwrap();
        let s = crate::settings::Settings::load(
            dir.path(),
            pumpkin_util::version::JavaMinecraftVersion::V_26_3,
        )
        .unwrap();
        assert_eq!(
            (s.network.java_port, s.network.bedrock_port),
            (25601, 19201)
        );
        assert_eq!(s.world.format, WorldFormat::Pump);
    }
}
