# Decisions

Reversible choices made without blocking on the owner (rule 9). Newest last. Each entry says what, why, and how to undo it.

## D-001 Working name: Gourd (2026-11-01, M0) — superseded by D-013

`{{SERVER_NAME}}` was still a placeholder, so M0 used "Gourd". It was picked without a collision check; one later found an existing Minecraft server project with that name (`Purdze/Gourd` on GitHub).

## D-002 Source and builds live in the proot rootfs, not shared storage (2026-11-01, M0)

The dev phone runs Ubuntu 26.04 in proot-distro inside Termux. `/mnt/sdcard` (Android shared storage) refuses both `symlink(2)` and the exec bit, so Cargo target dirs and server binaries cannot live there. The repo is at `/root/projects/tendril`. Undo: none needed; copy reports to shared storage if the owner wants them visible to Android apps.

## D-003 Git layout: `upstream` = Pumpkin-MC/Pumpkin, our work on `main` (2026-11-01, M0)

`master` mirrors `upstream/master` untouched so weekly merges stay trivial (rule 6); everything of ours lands on `main`. No GitHub fork exists yet: creating one needs the owner's GitHub account. Once it exists, add it as `origin` and push `main`.

## D-004 Base on upstream `master`, not the 0.2.1 tag (2026-11-01, M0)

`master` (645895098, 2026-10-31) already targets Bedrock 26.60 (protocol 2225) and has replaced RakNet with NetherNet, while the 0.2.1 tag targets Bedrock 26.51. Following `master` keeps us close to where upstream is going. Undo: `git checkout -B main 0.2.1+26.3-26.51` if `master` turns out too unstable.

## D-005 Upstream fixes are handed to the owner, never opened by the agent (2026-11-01, M0)

Rule 6 says fixes that belong in Pumpkin go upstream as PRs. Pumpkin's `AGENTS.md` says agents never open PRs, comments or reviews, with no exceptions, and requires a disclosure line and heading markers in agent-drafted descriptions. Both are satisfied by: the agent prepares a branch based on current `master` plus a description draft that follows `AGENTS.md`; the owner play-tests, attaches any required capture, and opens the PR. Queue so far: see "Upstream candidates" in `docs/m0-baseline.md`.

## D-006 Telemetry off by default (2026-11-01, M0)

Upstream writes `[telemetry] enabled = true` and sends a heartbeat to `market.pumpkinmc.org` every 300 s. Operators of a "one file, no surprises" server should opt in, not out. The launcher will write `enabled = false` into the translated Pumpkin config and expose `[updates]`-style opt-in in `server.toml`. Undo: flip the default in the config translator.

## D-007 Phone build profile: LTO off, `-j 2` (2026-11-01, M0)

Upstream's `[profile.release]` uses fat LTO with `codegen-units = 1`; upstream CI itself overrides `CARGO_PROFILE_RELEASE_LTO=off` for PR builds. Phone builds use the same override plus `-j 2` (section 11), without touching upstream's `Cargo.toml`. A `dist` profile with LTO for CI release artifacts comes with M7 packaging.

## D-008 Baseline probe is a self-written Python bot (2026-11-01, M0)

Pumpkin's BotMark (MIT, speaks 26.3 Java and NetherNet Bedrock) is the obvious load tool, but building it was refused by this session's permission policy (third-party build scripts). M0 uses `bench/javaprobe.py`, a dependency-free offline-mode login probe whose packet IDs come from Pumpkin's generated 26.3 table. It only measures login timing; load tests need a multi-bot tool, so BotMark (or rust-mc-bot) needs the owner's go-ahead before M2.

## D-009 Pin Fabric Loader 0.20.1 instead of 0.19.5 (2026-11-01, M0)

`meta.fabricmc.net` marks 0.20.1 as the current stable loader; 0.19.5 is no longer flagged stable. Fabric API stays at 0.165.0+26.3 (newest on Modrinth, 2026-10-23). Re-check both when M6 starts.

## D-010 Bedrock target: protocol 2225 (26.60 and 26.61) (2026-11-01, M0)

Mojang's `bedrock-protocol-docs` lists 26.61 (2026-10-29, the current BDS release) with the same protocol as 26.60, which master already targets. The previous minor line (26.50.x) required by section 8 is M3 work.

## D-011 RSS is reported together with VmSwap (2026-11-01, M0)

On the phone, Android pushed an idle server's pages to swap and VmRSS read 3.4 MB. The harness now records VmSwap, flags runs with more than 1 MiB swapped, and reports RSS + swap beside RSS. The budget is still checked against VmRSS (section 4), but only on runs with no swap.

## D-012 M0 baseline uses official upstream binaries (2026-11-01, M0)

Local builds were refused by the session's permission policy. The `nightly` release is built from the same commit as `main` (6458950) with the full release profile, so it measures our fork as it stands. Its digests come from the GitHub release API. From M1 on, every number must come from our own builds.

## D-013 Name: Tendril (2026-11-01, M1)

The owner asked for the best final name. A tendril is the part of a pumpkin vine that reaches out and holds on, which is what this server does: Java and Bedrock players, Paper plugins and Fabric mods, phones and PCs, on top of Pumpkin. Collision check on 2026-11-01: no Minecraft server project with that name on GitHub, only unrelated mods with "tendril" in the title on Modrinth. The crates.io name `tendril` is taken by an unrelated string library, so the Rust package is `tendril-launcher` and the binary is `tendril`. The display name lives in one constant (`NAME` in `launcher/setup/src/lib.rs`).

## D-014 The launcher replaces Pumpkin's binary, not its crates (2026-11-01, M1)

`launcher/` builds the `tendril` binary against `pumpkin-core` the same way `crates/pumpkin/src/main.rs` does: same rustls provider, panic hook, signal handling and `PumpkinServer` calls, but with Tendril's config, profile-sized thread pools and startup report. Upstream code is untouched apart from one line in the workspace `members`. `launcher/src/process.rs` holds the mirrored panic and signal code, and the test `upstream_main_unchanged` (in `tendril-setup`) fails whenever upstream's `main.rs` changes, so every merge forces a review of the copy. Undo: delete `launcher/` and the members line.

## D-015 View and simulation distance default to the profile (2026-11-01, M1)

Section 12 shows `view_distance = 8` and `simulation_distance = 6` set in `server.toml`, which would override the Android and low-RAM profiles (6 and 4) on every machine. The generated file leaves both commented out: unset means "profile default", and a set value always wins.

## D-016 `--offline` flag for one run (2026-11-01, M1)

The M1 gate wants a scripted bot to join from an empty folder with one command, but the default is `online_mode = true` and a bot has no Microsoft account. `tendril --accept-eula --offline` turns off account checks and encryption for that run only and logs a warning. It also covers LAN play without internet. The test still runs the plain default command, checking files and a status ping.

## D-017 `plugins/` is shared by Pumpkin and Paper plugins (2026-11-01, M1)

Pumpkin loads its own `.wasm` plugins from `plugins/`, and section 7.2 puts Paper `.jar` files there too. Tier selection only counts `.jar` files, so both kinds can live in one folder. The Bridge tier will map `plugins/*.jar` into PatchBukkit's own directory.

## D-018 Phone thread caps are partial (2026-11-01, M1)

The Android and low-RAM profiles size the Tokio runtime and Rayon's global pool to 2 threads. Pumpkin's simulation pool (all CPUs) and compute pool (half the CPUs, 2 to 8) are sized inside `Server::new` from `available_parallelism`, with no config knob. Capping those needs either an upstream option (candidate U-4) or limiting the process's CPU affinity; decide with measurements in M2.

## D-019 Low-RAM profile below 3 GiB (2026-11-01, M1)

Off Android, hosts with under 3 GiB of RAM get the low-RAM profile (covers 1-2 GB VPS plans and Raspberry Pi boards). Android always gets the Android profile, which detects Termux, or proot-distro on top of Termux, through `/system/bin/linker64` and `/data/data/com.termux`.

## D-020 Local builds use a checked Mojang jar (2026-11-01, M1)

`pumpkin-data`'s build script downloads Mojang's 26.3 server jar to extract the vanilla datapack, and the phone's connection drops large downloads mid-way. `/root/build/build.sh` points `PUMPKIN_MINECRAFT_SERVER_JAR` at a copy fetched with resume and checked against the SHA-1 in Mojang's version metadata (`33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`). It stays outside the repo and is never redistributed. CI lets the build script download it.

## D-021 EULA acceptance stored in vanilla `eula.txt` (2026-11-01, M1)

First run asks once (or takes `--accept-eula`) and writes `eula=true` to `eula.txt` in the format the vanilla server reads, so the Compat tier can reuse the answer. Without a terminal and without the flag, Tendril refuses to start and says how to accept.

## D-022 Launcher split so its logic builds on the phone (2026-11-01, M1)

`pumpkin-data` is 91 MB of generated Rust. Compiling it with one job used about 2.6 GB of RAM plus compressed swap within 80 s and was still growing when the memory guard stopped it; a two-job build exhausted memory and Android restarted the whole environment. So `launcher/setup` (`tendril-setup`) holds the settings, first-run, profile, tier and CLI code and depends only on `pumpkin-config` and `pumpkin-util`, which build and test on the phone. `launcher/` (`tendril-launcher`, binary `tendril`) is only `main.rs` and the mirrored process code. The full binary is built by CI or a machine with at least 8 GB of RAM (estimate, not measured).

## D-023 Android builds target API 24 (2026-11-01, M1)

`sysinfo` 0.40 (upstream bump in 2709e8b7a) links `getifaddrs`, which Android's libc only provides from API 24; `cargo-ndk` defaults to API 21, so the link failed. API 24 (Android 7.0) is also the oldest Android Termux supports. CI passes `--platform 24`. Upstream's own Android release job will hit the same error once it runs (it was skipped on that commit).

## D-024 Pushed history is only added to, never rewritten (2026-11-01, M1)

Once a branch is on GitHub, fixes land as new commits; no force-pushes. Before a push, fixups are folded into the commits they belong to.
