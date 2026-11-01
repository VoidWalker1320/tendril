#![deny(clippy::unwrap_used)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::process::{ExitCode, exit};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use pumpkin_config::PumpkinConfig;
use pumpkin_core::data::VanillaData;
use pumpkin_core::plugin::loader::PluginLoader;
use pumpkin_core::{PumpkinServer, SERVER_EXIT_CODE};
use pumpkin_data::packet::CURRENT_MC_VERSION;
use pumpkin_wasm_host::WasmPluginLoader;
use pumpkin_world::{CURRENT_BEDROCK_MC_PROTOCOL, CURRENT_BEDROCK_MC_VERSION};
use tendril_setup::bench::{self, Target, bot::PacketIds};
use tendril_setup::cli::{self, Command, RunOptions};
use tendril_setup::first_run::{self, Prepared};
use tendril_setup::profile::{self, Host, Kind, Profile};
use tendril_setup::settings::{self, Settings, TierMode};
use tendril_setup::tier::{self, Selection};
use tendril_setup::{DISCLAIMER, NAME, report, say};
use tracing::{error, info, warn};

mod process;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let started = Instant::now();
    match cli::parse(std::env::args().skip(1)) {
        Ok(Command::Run(options)) => run(&options, started),
        Ok(Command::Bench(options)) => run_bench(&options),
        Ok(Command::Help) => {
            say(cli::USAGE);
            ExitCode::SUCCESS
        }
        Ok(Command::Version) => {
            say(&format!(
                "{NAME} {VERSION} (Java {CURRENT_MC_VERSION}, Bedrock {CURRENT_BEDROCK_MC_VERSION})\n{DISCLAIMER}"
            ));
            ExitCode::SUCCESS
        }
        Err(message) => {
            say(&format!("{NAME}: {message}"));
            ExitCode::from(2)
        }
    }
}

/// Everything decided before the server starts, kept for the ready report.
struct Plan {
    started: Instant,
    prepared: Prepared,
    settings: Settings,
    offline: bool,
    host: Host,
    profile: Profile,
    selection: Selection,
}

fn run(options: &RunOptions, started: Instant) -> ExitCode {
    process::install_panic_hook();
    let plan = match plan(options, started) {
        Ok(plan) => plan,
        Err(message) => {
            say(&format!("{NAME}: {message}"));
            return ExitCode::FAILURE;
        }
    };
    let config = plan.settings.to_pumpkin(&plan.profile);

    // Same named global pool as upstream, sized by the profile.
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(plan.profile.worker_threads)
        .thread_name(|i| format!("Rayon-Worker-{i}"))
        .build_global();
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(plan.profile.worker_threads)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            say(&format!("{NAME}: cannot start the async runtime: {err}"));
            return ExitCode::FAILURE;
        }
    };
    runtime.block_on(serve(config, &plan))
}

fn plan(options: &RunOptions, started: Instant) -> Result<Plan, String> {
    let dir =
        std::env::current_dir().map_err(|e| format!("cannot read the current folder: {e}"))?;
    let prepared = first_run::prepare(&dir, options.accept_eula).map_err(|e| e.to_string())?;
    let mut settings = Settings::load(&dir, CURRENT_MC_VERSION).map_err(|e| e.to_string())?;
    if options.offline {
        settings.server.online_mode = false;
    }
    let mode = options.tier.unwrap_or(settings.tiers.mode);
    let selection = tier::select(
        mode,
        tier::count_jars(&dir.join("plugins")),
        tier::count_jars(&dir.join("mods")),
    );
    if mode != TierMode::Auto && !selection.wanted.available() {
        return Err(format!(
            "the {} tier is not built yet (milestone {}); only lite runs today",
            selection.wanted.name(),
            selection.wanted.milestone()
        ));
    }
    let host = profile::detect_host();
    let profile = profile::choose(settings.server.profile, &host);
    Ok(Plan {
        started,
        prepared,
        settings,
        offline: options.offline,
        host,
        profile,
        selection,
    })
}

async fn serve(config: PumpkinConfig, plan: &Plan) -> ExitCode {
    // reqwest is built with `rustls-no-provider`; upstream picks ring before any client exists.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let vanilla_data = VanillaData::load();
    pumpkin_core::init_logger(&config.advanced);
    log_banner(plan);

    tokio::spawn(async {
        if let Err(err) = process::handle_signals().await {
            error!("Unable to setup signal handlers: {err}");
        }
    });

    let plugin_loaders: Vec<Arc<dyn PluginLoader>> = vec![Arc::new(WasmPluginLoader::new(
        config.advanced.plugins.verify_signatures,
    ))];
    let server = PumpkinServer::new(
        config.basic,
        config.advanced,
        config.telemetry,
        vanilla_data,
        plugin_loaders,
        true,
    )
    .await
    .unwrap_or_else(|err| {
        error!("{err}");
        exit(1);
    });
    // Time spent waiting for the operator to confirm plugin permissions isn't startup time.
    let plugin_wait = server.init_plugins().await;
    log_ready(plan, plugin_wait);

    server.start().await;
    info!("The server has stopped.");
    exit(SERVER_EXIT_CODE.load(Ordering::Acquire));
}

fn log_banner(plan: &Plan) {
    info!(
        "Starting {NAME} {VERSION}: Java {CURRENT_MC_VERSION} (protocol {}) | Bedrock {CURRENT_BEDROCK_MC_VERSION} (protocol {CURRENT_BEDROCK_MC_PROTOCOL})",
        CURRENT_MC_VERSION.protocol_version()
    );
    info!("{DISCLAIMER}");
    info!(
        "Runs on Pumpkin (GPL-3.0, under heavy development): https://github.com/Pumpkin-MC/Pumpkin"
    );
    if plan.prepared.created_config {
        info!(
            "First run: wrote {} with defaults; edit it and restart to change settings",
            settings::FILE_NAME
        );
    }
    if plan.offline {
        warn!(
            "--offline: no account checks and no encryption for this run; anyone can join under any name"
        );
    }
    if let Ok(min) = settings::parse_min_java_version(
        &plan.settings.network.min_java_version,
        CURRENT_MC_VERSION,
    ) && min != CURRENT_MC_VERSION
    {
        info!(
            "min_java_version is {}, but only {CURRENT_MC_VERSION} clients can join until multi-version support lands (M4)",
            plan.settings.network.min_java_version
        );
    }
}

fn log_ready(plan: &Plan, plugin_wait: Duration) {
    let p = &plan.profile;
    info!(
        "Profile: {} ({}): view distance {}, simulation distance {}, {} worker thread{} ({}, {} CPUs)",
        p.kind.name(),
        p.reason,
        plan.settings
            .world
            .view_distance
            .unwrap_or(p.view_distance.get()),
        plan.settings
            .world
            .simulation_distance
            .unwrap_or(p.simulation_distance.get()),
        p.worker_threads,
        if p.worker_threads == 1 { "" } else { "s" },
        plan.host
            .total_ram_bytes
            .map_or_else(|| "RAM unknown".to_string(), profile::gib),
        plan.host.cpus,
    );
    let wanted = plan.selection.wanted;
    if wanted.available() {
        info!(
            "Tier: {} ({}), see docs/tiers.md",
            wanted.name(),
            plan.selection.reason
        );
    } else {
        warn!(
            "Tier: Lite. {} need the {} tier, which is not built yet (milestone {}), so they will not load. See docs/tiers.md",
            plan.selection.reason,
            wanted.name(),
            wanted.milestone()
        );
    }

    let net = &plan.settings.network;
    let lan = report::lan_ipv4();
    let host = lan.map_or_else(|| "localhost".to_string(), |ip| ip.to_string());
    info!(
        "Java Edition: {host}:{} (this device: localhost:{})",
        net.java_port, net.java_port
    );
    if net.bedrock_enabled {
        info!("Bedrock Edition: {host}:{} (TCP and UDP)", net.bedrock_port);
    }

    // From the start of main; exec-to-login is measured from outside by bench/bench_m0.py.
    let elapsed = plan.started.elapsed().saturating_sub(plugin_wait);
    let rss = report::rss_bytes().map_or_else(|| "unknown".to_string(), report::mib);
    info!("Ready in {} ms, RSS {rss}", elapsed.as_millis());

    if p.kind == Kind::Android {
        info!("On Android, run `termux-wake-lock` so the phone doesn't suspend the server");
        info!(
            "Mobile data is usually behind carrier NAT: players outside your Wi-Fi need a tunnel or relay to reach you"
        );
    }
    if lan.is_none() {
        warn!("No network address found: only this device can connect");
    }
}

fn run_bench(options: &bench::Options) -> ExitCode {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(err) => {
            say(&format!("{NAME}: cannot find my own executable: {err}"));
            return ExitCode::FAILURE;
        }
    };
    let target = Target {
        exe: &exe,
        ids: packet_ids(),
        version: VERSION,
        commit: option_env!("GITHUB_SHA"),
    };
    match bench::run(target, options, &mut std::io::stdout()) {
        Ok(_) => ExitCode::SUCCESS,
        Err(err) => {
            say(&format!("{NAME}: bench failed: {err}"));
            ExitCode::FAILURE
        }
    }
}

/// The bench bot's packet IDs, taken from the protocol this binary serves.
const fn packet_ids() -> PacketIds {
    use pumpkin_data::packet::{clientbound as cb, serverbound as sb};
    PacketIds {
        protocol: CURRENT_MC_VERSION.protocol_version(),
        sb_handshake: sb::handshake::INTENTION.0,
        sb_status_request: sb::status::STATUS_REQUEST.0,
        cb_status_response: cb::status::STATUS_RESPONSE.0,
        sb_login_hello: sb::login::HELLO.0,
        sb_login_custom_query_answer: sb::login::CUSTOM_QUERY_ANSWER.0,
        sb_login_ack: sb::login::LOGIN_ACKNOWLEDGED.0,
        sb_login_cookie_response: sb::login::COOKIE_RESPONSE.0,
        cb_login_disconnect: cb::login::LOGIN_DISCONNECT.0,
        cb_login_encryption: cb::login::HELLO.0,
        cb_login_finished: cb::login::LOGIN_FINISHED.0,
        cb_login_compression: cb::login::LOGIN_COMPRESSION.0,
        cb_login_custom_query: cb::login::CUSTOM_QUERY.0,
        cb_login_cookie_request: cb::login::COOKIE_REQUEST.0,
        sb_cfg_cookie_response: sb::config::COOKIE_RESPONSE.0,
        sb_cfg_finish: sb::config::FINISH_CONFIGURATION.0,
        sb_cfg_keep_alive: sb::config::KEEP_ALIVE.0,
        sb_cfg_pong: sb::config::PONG.0,
        sb_cfg_resource_pack: sb::config::RESOURCE_PACK.0,
        sb_cfg_known_packs: sb::config::SELECT_KNOWN_PACKS.0,
        sb_cfg_accept_code_of_conduct: sb::config::ACCEPT_CODE_OF_CONDUCT.0,
        cb_cfg_cookie_request: cb::config::COOKIE_REQUEST.0,
        cb_cfg_disconnect: cb::config::DISCONNECT.0,
        cb_cfg_finish: cb::config::FINISH_CONFIGURATION.0,
        cb_cfg_keep_alive: cb::config::KEEP_ALIVE.0,
        cb_cfg_ping: cb::config::PING.0,
        cb_cfg_resource_pack_push: cb::config::RESOURCE_PACK_PUSH.0,
        cb_cfg_known_packs: cb::config::SELECT_KNOWN_PACKS.0,
        cb_cfg_code_of_conduct: cb::config::CODE_OF_CONDUCT.0,
        sb_accept_teleport: sb::play::ACCEPT_TELEPORTATION.0,
        sb_keep_alive: sb::play::KEEP_ALIVE.0,
        sb_pong: sb::play::PONG.0,
        sb_chunk_batch_received: sb::play::CHUNK_BATCH_RECEIVED.0,
        sb_player_loaded: sb::play::PLAYER_LOADED.0,
        cb_disconnect: cb::play::DISCONNECT.0,
        cb_keep_alive: cb::play::KEEP_ALIVE.0,
        cb_login: cb::play::LOGIN.0,
        cb_ping: cb::play::PING.0,
        cb_player_position: cb::play::PLAYER_POSITION.0,
        cb_chunk_batch_finished: cb::play::CHUNK_BATCH_FINISHED.0,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn phone_fixture_matches_generated_ids() {
        // tendril-setup's examples and tests use IDS_26_3; it must track pumpkin-data.
        assert_eq!(super::packet_ids(), tendril_setup::bench::bot::IDS_26_3);
    }
}
