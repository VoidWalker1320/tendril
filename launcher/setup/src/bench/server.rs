//! A Tendril server running as a child process, driven through its console.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(10);

pub struct ServerProcess {
    child: Child,
    stdin: ChildStdin,
    lines: Arc<Mutex<Vec<String>>>,
    pub started: Instant,
}

/// Memory and CPU of the server process at one moment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub rss_bytes: u64,
    /// Pages Android or Linux pushed to swap; `None` where the OS doesn't say.
    pub swap_bytes: Option<u64>,
    pub cpu_ms: u64,
}

impl ServerProcess {
    /// Starts `exe --accept-eula --offline` in `dir`.
    ///
    /// # Errors
    /// Fails if the process cannot be started.
    pub fn spawn(exe: &Path, dir: &Path) -> io::Result<Self> {
        let started = Instant::now();
        let mut child = Command::new(exe)
            .args(["--accept-eula", "--offline"])
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("no stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("no stdout"))?;
        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&lines);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                sink.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(strip_ansi(&line));
            }
        });
        Ok(Self {
            child,
            stdin,
            lines,
            started,
        })
    }

    #[must_use]
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Number of console lines seen so far; pass it to `wait_for` to only look at newer ones.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Waits for a console line at index `from` or later that contains `needle`.
    ///
    /// # Errors
    /// Fails if the server exits or `timeout` passes first.
    pub fn wait_for(&mut self, needle: &str, from: usize, timeout: Duration) -> io::Result<String> {
        let deadline = Instant::now() + timeout;
        loop {
            {
                let lines = self.lines.lock().unwrap_or_else(PoisonError::into_inner);
                if let Some(line) = lines.iter().skip(from).find(|l| l.contains(needle)) {
                    return Ok(line.clone());
                }
            }
            self.check_alive()?;
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("no console line containing {needle:?}"),
                ));
            }
            std::thread::sleep(POLL);
        }
    }

    /// Waits until the Java port accepts connections; returns the time since spawn.
    ///
    /// # Errors
    /// Fails if the server exits or `timeout` passes first.
    pub fn wait_port(&mut self, addr: SocketAddr, timeout: Duration) -> io::Result<Duration> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
                return Ok(self.started.elapsed());
            }
            self.check_alive()?;
            std::thread::sleep(POLL);
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, "port never opened"))
    }

    /// Sends one console command.
    ///
    /// # Errors
    /// Fails if the server's stdin is closed.
    pub fn command(&mut self, command: &str) -> io::Result<()> {
        writeln!(self.stdin, "{command}")?;
        self.stdin.flush()
    }

    /// Sends `stop` and waits for a clean exit, killing the process after `timeout`.
    ///
    /// # Errors
    /// Fails if the process cannot be waited for.
    pub fn stop(mut self, timeout: Duration) -> io::Result<(ExitStatus, Duration)> {
        let asked = Instant::now();
        let _ = self.command("stop");
        while asked.elapsed() < timeout {
            if let Some(status) = self.child.try_wait()? {
                return Ok((status, asked.elapsed()));
            }
            std::thread::sleep(POLL);
        }
        self.child.kill()?;
        Ok((self.child.wait()?, asked.elapsed()))
    }

    /// Current memory and accumulated CPU time of the server process.
    #[must_use]
    pub fn usage(&self) -> Option<Usage> {
        let pid = sysinfo::Pid::from_u32(self.pid());
        let mut system = sysinfo::System::new().ok()?;
        system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&[pid]),
            false,
            sysinfo::ProcessRefreshKind::nothing()
                .with_memory()
                .with_cpu(),
        );
        let process = system.process(pid)?;
        Some(Usage {
            rss_bytes: process.memory(),
            swap_bytes: swap_bytes(self.pid()),
            cpu_ms: process.accumulated_cpu_time(),
        })
    }

    fn check_alive(&mut self) -> io::Result<()> {
        self.child.try_wait()?.map_or(Ok(()), |status| {
            Err(io::Error::other(format!("server exited early: {status}")))
        })
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn swap_bytes(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let kib: u64 = status
        .lines()
        .find_map(|l| l.strip_prefix("VmSwap:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some(kib * 1024)
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
const fn swap_bytes(_pid: u32) -> Option<u64> {
    None
}

/// Removes terminal colour codes (`ESC [ ... letter`) from a console line.
#[must_use]
pub fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi_codes_are_removed() {
        assert_eq!(
            strip_ansi("\u{1b}[32mINFO\u{1b}[0m Ready in \u{1b}[1;33m5\u{1b}[0m ms"),
            "INFO Ready in 5 ms"
        );
        assert_eq!(strip_ansi("plain"), "plain");
    }
}
