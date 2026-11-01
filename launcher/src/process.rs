//! Panic and signal handling, mirrored from `crates/pumpkin/src/main.rs`.
//!
//! The launcher replaces Pumpkin's binary, so it has to do the same process-level setup.
//! `upstream_main_unchanged` in `tendril-setup` fails when upstream's `main.rs` changes; review
//! this file and `main.rs` against it on every merge, then update `UPSTREAM_MAIN_FNV1A`.

use std::backtrace::{Backtrace, BacktraceStatus};
use std::panic::PanicHookInfo;
use std::sync::OnceLock;
use std::sync::atomic::Ordering;
use std::thread::{self, ThreadId};

use pumpkin_core::crash::{CrashReport, FullBacktrace};
use pumpkin_core::{CRASH_REPORT, SERVER_IS_STOPPING, stop_or_exit_server, stop_server};
use pumpkin_util::text::TextComponent;
use pumpkin_util::text::color::{Color, NamedColor};
use tracing::warn;

static MAIN_THREAD: OnceLock<ThreadId> = OnceLock::new();

/// Records the main thread and installs the panic hook. Call first thing in `main`.
pub fn install_panic_hook() {
    let _ = MAIN_THREAD.set(thread::current().id());
    std::panic::set_hook(Box::new(handle_panic));
}

fn handle_interrupt() {
    warn!(
        "{}",
        TextComponent::text("Received interrupt signal; stopping server...")
            .color_named(NamedColor::Red)
            .to_pretty_console()
    );
    stop_or_exit_server();
}

fn panic_message<'a>(panic_info: &'a PanicHookInfo<'_>) -> &'a str {
    let payload = panic_info.payload();
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("<unknown>")
}

fn handle_panic(panic_info: &PanicHookInfo<'_>) {
    // Capture here so the backtrace doesn't start inside CrashReport::new.
    let crash_report = {
        let captured_backtrace = Backtrace::capture();
        let full_backtrace = if captured_backtrace.status() == BacktraceStatus::Captured {
            FullBacktrace::Captured
        } else {
            FullBacktrace::ForceCaptured(Backtrace::force_capture())
        };
        CrashReport::new(panic_info, captured_backtrace, full_backtrace)
    };

    if is_main_thread() {
        // The main thread can't shut down gracefully after panicking, but can still report.
        if let Some(crash_report) = try_set_crash_report(crash_report) {
            crash_report.print_to_console();
            crash_report.save_and_log();
            tracing::error!(
                "{}",
                TextComponent::text("Aborting due to the main thread panicking.")
                    .color(Color::Named(NamedColor::Red))
                    .to_pretty_console()
            );
        } else {
            tracing::error!(
                "{}: {}",
                TextComponent::text(
                    "The main thread panicked while stopping the server; aborting."
                )
                .color(Color::Named(NamedColor::Red))
                .bold()
                .to_pretty_console(),
                panic_message(panic_info)
            );
        }
        std::process::exit(1);
    }

    if try_set_crash_report(crash_report).is_some() {
        stop_server();
    } else {
        tracing::error!(
            "{}: {}",
            TextComponent::text("Encountered panic while shutting down")
                .color(Color::Named(NamedColor::Red))
                .bold()
                .to_pretty_console(),
            panic_message(panic_info)
        );
    }
}

fn is_main_thread() -> bool {
    Some(&thread::current().id()) == MAIN_THREAD.get()
}

/// `Some` for the first panic, which must be logged and saved; `None` for later ones.
fn try_set_crash_report(crash_report: CrashReport) -> Option<&'static CrashReport> {
    if !SERVER_IS_STOPPING.load(Ordering::Acquire) && CRASH_REPORT.set(crash_report).is_ok() {
        CRASH_REPORT.get()
    } else {
        None
    }
}

/// Waits for Ctrl+C (and on Unix SIGHUP/SIGTERM), then stops the server.
///
/// # Errors
/// Fails if the signal handlers cannot be registered.
#[cfg(unix)]
pub async fn handle_signals() -> std::io::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut hangup = signal(SignalKind::hangup())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let received = tokio::select! {
        received = interrupt.recv() => received,
        received = hangup.recv() => received,
        received = terminate.recv() => received,
    };
    if received.is_some() {
        handle_interrupt();
    }
    Ok(())
}

/// Waits for Ctrl+C, the console window closing or a system shutdown, then stops the server.
///
/// Windows ends the process about 5 s after a close or shutdown event, so a large world may not
/// finish saving; `stop` in the console is the safe way out.
///
/// # Errors
/// Fails if the console handlers cannot be registered.
#[cfg(windows)]
pub async fn handle_signals() -> std::io::Result<()> {
    use tokio::signal::windows::{ctrl_c, ctrl_close, ctrl_shutdown};
    let mut interrupt = ctrl_c()?;
    let mut close = ctrl_close()?;
    let mut shutdown = ctrl_shutdown()?;
    let received = tokio::select! {
        received = interrupt.recv() => received,
        received = close.recv() => received,
        received = shutdown.recv() => received,
    };
    if received.is_some() {
        handle_interrupt();
    }
    Ok(())
}

/// Waits for Ctrl+C, then stops the server.
///
/// # Errors
/// Fails if the Ctrl+C handler cannot be registered.
#[cfg(not(any(unix, windows)))]
pub async fn handle_signals() -> std::io::Result<()> {
    if tokio::signal::ctrl_c().await.is_ok() {
        handle_interrupt();
    }
    Ok(())
}
