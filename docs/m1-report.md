# M1 report: launcher, single `server.toml`, first run (2026-11-01)

Gate: "empty folder, one command, a Java 26.3 bot joins, on every CI target".

## Result

| Target | Built | Gate test (`bench/first_run_test.py`) | Where |
| --- | --- | --- | --- |
| Linux x86_64 (glibc) | yes | pass | GitHub Actions |
| Linux x86_64 musl | yes | pass | GitHub Actions |
| Linux aarch64 (glibc) | yes | pass | GitHub Actions |
| Linux aarch64 musl | yes | pass | GitHub Actions |
| Windows x86_64 | yes | pass | GitHub Actions |
| macOS arm64 | yes | pass | GitHub Actions |
| macOS x86_64 | yes (cross-compiled) | not run: no x86_64 macOS runner | GitHub Actions |
| Android aarch64 | yes (`cargo ndk`, API 24) | pass | the dev phone, with the CI artifact |

CI run 41318004683 on commit 8de1ca413 is green on every job. The gate test runs two scenarios in empty folders:

1. `tendril --accept-eula` (defaults, online mode): writes `server.toml`, `eula.txt`, `plugins/`, `mods/` and `logs/`, writes no `pumpkin.toml`, answers a status ping as protocol 777, stops cleanly on `stop`.
2. `tendril --accept-eula --offline`: the probe bot spawns (D-016 explains why the bot needs offline mode).

## Measured

One fresh-world run per target, so these are smoke numbers, not section 14 benchmarks. "Ready" is from the start of `main` to the server listening; "spawn" is from process exec until the bot gets its spawn position in a brand-new world, so it includes generating the spawn chunks.

| Target | Ready | RSS at ready | Exec to bot spawn |
| --- | --- | --- | --- |
| Linux x86_64 | 38 ms | 72.9 MiB | 1.64 s |
| Linux x86_64 musl | 34 ms | 68.9 MiB | 2.89 s |
| Linux aarch64 | 19 ms | 131.6 MiB | 1.45 s |
| Linux aarch64 musl | 25 ms | 30.9 MiB | 1.15 s |
| macOS arm64 | 106 ms | 47.3 MiB | 1.84 s |
| Windows x86_64 | 159 ms | 25.2 MiB (working set) | 1.67 s |
| Android phone (proot, CI binary, LTO off) | 356 ms | 65.8 MiB | 4.29 s |

CI numbers come from run 41315195831 (same code before the Android fix); raw lines in `bench/results/m1-2026-11-01/`. The phone is the M0 machine (2x A75 + 6x A55, 3.6 GiB RAM), under proot.

## Not done or partial

- The phone cannot compile Pumpkin's core: `pumpkin-data` needs more memory than the phone has free, even for `cargo check` (D-022). The phone builds and tests `tendril-setup`; full binaries come from CI.
- macOS x86_64 is built but never run.
- Phone profile thread caps cover Tokio and Rayon's global pool, not Pumpkin's simulation and compute pools (D-018).
- `tendril doctor`, `tendril bench`, `tendril plugin` and `[updates] check` do not exist yet.
- CI binaries are built without LTO (172-203 MB); a `dist` profile with LTO comes with M7.
- Every Pumpkin binary embeds Mojang's vanilla datapack data (`include_str!` in `pumpkin-data`). That conflicts with rule 5 for released binaries and needs a decision before M7.

## Upstream candidates

- U-4: a config knob for the sizes of Pumpkin's simulation and compute pools.
- U-5: Android release builds will fail on `sysinfo` 0.40 (`getifaddrs` needs API 24, `cargo ndk` defaults to 21). Upstream skipped that job on 2709e8b7a because macOS tests failed first.
