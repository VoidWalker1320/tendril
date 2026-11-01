# M0 baseline (2026-11-01)

Recon for Tendril (called Gourd during M0): upstream state, verified versions and a measured baseline before any of our code exists.

## Machine and method

| | |
| --- | --- |
| Device | Android phone, 2x Cortex-A75 @ 2.0 GHz + 6x Cortex-A55 @ 1.8 GHz, 3.6 GiB RAM (0.7-1.1 GiB available during runs, 1.6-1.8 GiB swap in use by other apps). Model and Android version are not readable from proot (`getprop` is blocked). |
| Environment | Termux, proot-distro Ubuntu 26.04.1 aarch64, Python 3.14. **Every number below ran under proot**, which intercepts syscalls with ptrace; a native Termux run will be somewhat faster. |
| Binaries | Official upstream artifacts, checksums verified (`bench/results/m0-2026-11-01/sha256-*.txt`): release `0.2.1+26.3-26.51` (224f9f8) and the `nightly` release built from master **6458950** with the full LTO release profile. 6458950 is exactly the commit `main` sits on, so "master" below is our fork today. |
| Why prebuilt | Building Pumpkin from source in this session was refused by its permission policy (see "Not done"). |
| Config | Upstream defaults (`pump` chunks, view distance 16, three dimensions) except: seed `20261101`, Java and Bedrock `online_mode = false`, `encryption = false`, telemetry off, no colour/TTY. |
| Command | `python3 -I bench/bench_m0.py <binary> <empty-dir> 3 60` |
| Startup | Process exec until the probe bot (`bench/javaprobe.py`, offline mode, protocol 777) receives the clientbound Player Position packet in play state, i.e. it has spawned. The probe retries from exec, so the number includes binding the port. |
| Idle | After that first join the bot disconnects; 1 s later a 60 s window starts with 0 players. VmRSS, VmSwap and CPU time come from `/proc/<pid>`. |
| Runs | Run 0 creates the world (reported separately); runs 1-3 reuse it. Medians of runs 1-3. Configurations were interleaved (master, 0.2.1, master, ...) so drift in phone state hits both. No run had swapped-out pages at sample time (`swap_contaminated_runs = 0`). |

## Results (median of 3, existing world)

| Build | Binary MB | Port open ms | "Started" log ms | Play login ms | **Spawned ms** | **Idle RSS MiB** | Idle CPU % of one core | Fresh world: spawned ms | Fresh world: idle RSS MiB |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0.2.1 glibc | 116.1 | 258 | 302 | 398 | **605** | **79.9** | 12.5 | 3929 | 117.5 |
| master glibc | 120.0 | 360 | 397 | 577 | **1426** | **87.6** | 11.7 | 4638 | 119.1 |
| 0.2.1 android (bionic) | 117.1 | 291 | 330 | 428 | **677** | **76.7** | 15.7 | 4702 | 92.0 |
| master android (bionic) | 121.2 | 326 | 360 | 502 | **1589** | **89.9** | 13.2 | 4051 | 105.3 |
| 0.2.1 musl | 102.1 | 267 | 361 | 585 | **1010** | **41.3** | 81.9 | 34276 | 66.4 |
| master musl | 105.0 | 256 | 297 | 555 | **1784** | **42.5** | 20.3 | 4836 | 32.1 * |

\* that fresh-world sample had 25 MiB swapped out, so its RSS is too low; fresh-world runs are not in the medians.

An earlier round with a harness that did not yet record VmSwap gave 0.2.1 glibc 501 ms / 75.7 MiB, so expect roughly 10-15% run-to-run spread on this phone.

### Against the Lite budgets (section 4)

| Budget | Status on this phone |
| --- | --- |
| Idle RSS ≤ 100 MB, existing world | Met by every build. Master android, the Termux target, has 10 MiB of headroom. |
| Startup ≤ 3 s on a mid-range phone | Met by every build (master android 1.6 s). |
| Startup ≤ 1 s on a desktop | Not measurable here; needs a desktop or CI runner. |
| Steady 20 TPS / MSPT | Not measured (needs a multi-bot load tool, see "Not done"). |

## Findings

1. **Master regressed against 0.2.1 in one week.** On both glibc and bionic: +820-910 ms to spawn and +8-13 MiB idle RSS. Port open and ready log moved by under 100 ms; the time is spent between the play Login packet and Player Position (master 850-1090 ms, 0.2.1 140-260 ms). Bisecting needs local builds. 44 commits sit between them; the likeliest are b70c7e1e6 (decouple tick simulation, compute pools and Tokio runtime), cf4e476b6 (DynamicRegistryManager with runtime datapack loaders) and 5e9101f35 (server lifecycle decoupled for embedding). Weekly upstream merges (rule 6) can therefore eat the 100 MB budget, so nightly regression CI (section 14) needs to come early.
2. **musl trades CPU for memory.** It has half the RSS (41-43 MiB) but the slowest spawn, and on 0.2.1 world generation was 9x slower (34 s vs 3.9 s) with idle CPU at 82%. Master's worldgen work (eba538f28) closed most of that gap. Section 11's "static musl binary" needs an allocator comparison (mimalloc) before it becomes the Linux default.
3. **Idle is not idle.** 12-20% of a core with 0 players on every build. No budget exists for it, but on a phone it costs battery and heat. Propose adding an idle-CPU budget in M2.
4. **First boot is over budget.** After generating a new world, glibc builds sit at 117-119 MiB. The budget is defined on an existing world, but this is what a new operator sees first.
5. **The binary is not small.** 116-121 MB: `.text` 47 MB, symbol tables 24 MB (strippable with `strip = "symbols"`), dynamic relocations 14 MB plus `.data.rel.ro` 11 MB. Both are written at load and likely add private RSS; test in M2. Without LTO (the `canary` artifacts) it is 172-203 MB.
6. **Telemetry is on by default.** Upstream heartbeats to `market.pumpkinmc.org` every 300 s (see D-006).
7. **Bedrock moved to NetherNet only.** On master (87531ab20, 2026-10-31), RakNet is gone. The server listens on TCP 19132 (signaling), UDP 19132 (ICE and status) and UDP 7551 (LAN discovery). Section 8's "UDP 19132" now also needs TCP 19132 in firewall docs, the Docker image and the Pterodactyl egg. A real client join has not been verified (no Bedrock client or runnable NetherNet bot here).
8. **Bedrock accepts exactly one protocol.** That is 2225 (26.60, and 26.61 shares it). Section 8 asks for the previous minor line too (26.50.x); that is M3 work.

## Upstream candidates (owner opens them, see D-005)

- **U-1 Bedrock OIDC keys are fetched once with no retry.** In `crates/pumpkin-core/src/server/mod.rs`, a failed fetch stores an empty key set in a `OnceCell`, and `net/bedrock/login/mod.rs` then rejects every Xbox-authenticated login until restart. Seen on the first boot here: `Failed to fetch Bedrock OIDC keys ... error sending request`, because a cold DNS lookup on the phone took about 5.6 s against a 5000 ms timeout. Mojang chat-key fetching has the same pattern (`ArcSwap` empty vec), but only runs with `allow_chat_reports = true`.
- **U-2 Join and RSS regression 0.2.1 → 6458950** (finding 1), as an issue with the numbers above, once bisected.
- **U-3 Docs.** `docs.pumpkinmc.org/about/benchmarks` returns 404, yet pumpkinmc.org still quotes "boots in 5 ms" and "~100 MB" from it.

## Upstream parity tracker

pumpkinmc.org/tracker: "569 entries read from Pumpkin 919cac17f50d for Minecraft 26.3-26.60, 2026-10-31". Implemented 400 (70%), partial 161 (28%), planned 8 (1%). Per category: entities 160 (29%), blocks 201 (81%), items 55 (93%), commands 96 (95%), redstone 27 (85%), combat 14 (93%), world 16 (81%).

## Version check (all fetched 2026-11-01)

| Component | Prompt said | Found | Source | Action |
| --- | --- | --- | --- | --- |
| Java Edition | 26.3 / 777 | 26.3 / 777, no 26.3.x hotfix; 26.4-snapshot-6 out | piston-meta version manifest; PrismarineJS minecraft-data `protocolVersions.json` | none |
| Java range | 1.20-26.3 | 763 (1.20/1.20.1), 764, 765, 766, 767, 768, 769, 770, 771, 772, 773, 774 (1.21.11), 775 (26.1.x), 776 (26.2), 777 (26.3) | minecraft-data, matches Pumpkin `version.rs`; the wiki protocol page returned 404 to the fetcher | none |
| Bedrock | latest | **26.61** release (BDS 1.26.61.1) = protocol 2225, same as 26.60; 26.70 preview = 2239 | Mojang BDS download API; Mojang `bedrock-protocol-docs` releases | target 2225 |
| Pumpkin | 0.2.0+26.3-26.51 | **0.2.1+26.3-26.51** (2026-10-24); master 6458950 targets Bedrock 26.60 | GitHub releases | based on master (D-004) |
| PatchBukkit | release matched to Pumpkin | 0.2.1 (2026-10-24); **no Android build** (Linux/macOS/Windows only) | GitHub releases | Bridge on Termux needs our own build |
| Paper | 26.3 beta | **26.3 STABLE** since build 175 (2026-10-11); latest stable 235 (2026-10-31) | fill.papermc.io v3 | use stable |
| Fabric Loader | 0.19.5 | **0.20.1** stable | meta.fabricmc.net | switched (D-009) |
| Fabric API | 0.165.0+26.3 | 0.165.0+26.3 (2026-10-23) | Modrinth | none |
| Fabric Loom | from fabricmc.net | 1.19.3 (`net.fabricmc.fabric-loom`) | maven.fabricmc.net | pin 1.19.3 |
| Geyser | latest for 26.3 | 2.12.1 b1280 (2026-10-31); Bedrock 26.40-26.60; "Remove RakNet-only transport" in the latest build | download.geysermc.org, README | none |
| Floodgate (Fabric) | latest | 2.2.7-b69 (2026-09-17) | Modrinth | none |
| FabricProxy-Lite | verify 26.3 | v2.12.0 lists 26.3 | Modrinth | none |
| ViaVersion / ViaBackwards (Fabric) | verify 26.3 | 5.12.3-SNAPSHOT, beta only; ViaFabric 0.4.22 alpha | Modrinth | treat as unstable |
| Fabric + Bukkit hybrid | check | **none for 26.3**: Arclight has only an experimental 26.1.2 build; the Banner repo is gone | GitHub | M8 FabricHost stays the only route |
| Lithium / FerriteCore | | mc26.3-0.26.3-fabric / 9.0.0-fabric, both list 26.3 | Modrinth | none |
| Paper acceptance set | latest for 26.3 | LuckPerms 5.5.71 and WorldEdit 7.4.6 list 26.3. PlaceholderAPI 2.12.3, CoreProtect 24.1 and VaultUnlocked 2.20.2 list up to 26.2 only. **EssentialsX's** last release is 2.22.0 (2026-05-31); 26.x work exists only in dev builds | Modrinth, Hangar, GitHub | test dev builds or substitute in M5 |
| JDK | 25 (Termux 25.0.3) | proot Ubuntu: 25.0.4.1. Termux repo: openjdk-25 **25.0.5**. Termux on this phone has 17 and 21 installed, not 25 | Termux apt index | `pkg install openjdk-25` before M5 |
| Rust | upstream pin | `rust-toolchain.toml` = `stable`; MSRV raised to 1.97.0 (2026-10-31); stable is 1.99.1; Termux rust 1.99.1 | rustup | installed 1.99.1 |
| pumpkin-java-multiversion | build on it | claims 1.7.2-26.3; last commit 2026-09-22. Cargo says MIT OR Apache-2.0, there is no LICENSE file, and it bundles GPL-3.0 ViaBackwards/ViaRewind data (`assets/NOTICE.md`) | repo | fine inside a GPL fork; attribution required |
