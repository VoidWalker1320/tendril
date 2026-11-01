# Decisions

Reversible choices made without blocking on the owner (rule 9). Newest last. Each entry says what, why, and how to undo it.

## D-001 Working name: Gourd (2026-11-01, M0)

`{{SERVER_NAME}}` was still a placeholder. "Gourd" is short, unused by any known server, ties back to Pumpkin without reusing its name, and keeps "Minecraft" out of the product name (section 15). From M1 on the name lives in one constant in the launcher crate; renaming is a one-line change there plus the binary name in `Cargo.toml`.

## D-002 Source and builds live in the proot rootfs, not shared storage (2026-11-01, M0)

The dev phone runs Ubuntu 26.04 in proot-distro inside Termux. `/mnt/sdcard` (Android shared storage) refuses both `symlink(2)` and the exec bit, so Cargo target dirs and server binaries cannot live there. The repo is at `/root/projects/gourd`. Undo: none needed; copy reports to shared storage if the owner wants them visible to Android apps.

## D-003 Git layout: `upstream` = Pumpkin-MC/Pumpkin, our work on `main` (2026-11-01, M0)

`master` mirrors `upstream/master` untouched so weekly merges stay trivial (rule 6); everything of ours lands on `main`. No GitHub fork exists yet: creating one needs the owner's GitHub account. Once it exists, add it as `origin` and push `main`.

## D-004 Base on upstream `master`, not the 0.2.1 tag (2026-11-01, M0)

`master` (645895098, 2026-10-31) already targets Bedrock 26.60 (protocol 2225) and has replaced RakNet with NetherNet, while the 0.2.1 tag targets Bedrock 26.51. Following `master` keeps us close to where upstream is going. Undo: `git checkout -B main 0.2.1+26.3-26.51` if `master` turns out too unstable.

## D-005 Upstream fixes are handed to the owner, never opened by the agent (2026-11-01, M0)

Rule 6 says fixes that belong in Pumpkin go upstream as PRs. Pumpkin's `AGENTS.md` says agents never open PRs, comments or reviews, with no exceptions, and requires a disclosure line and heading markers in agent-drafted descriptions. Both are satisfied by: the agent prepares a branch based on current `master` plus a description draft that follows `AGENTS.md`; the owner play-tests, attaches any required capture, and opens the PR. Queue so far: see "Upstream candidates" in `docs/m0-baseline.md`.

## D-006 Telemetry off by default in Gourd (2026-11-01, M0)

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
