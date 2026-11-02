# M2 report: the Lite tier hits its budgets (2026-11-02)

Gate: idle RSS ≤ 100 MB, startup ≤ 1 s on a desktop and ≤ 3 s on a phone, median of 3 runs.

## Result

| Machine | Build | Startup (exec to Java bot spawned) | Idle RSS after 60 s | Verdict |
| --- | --- | --- | --- | --- |
| GitHub runner, Linux x86_64, 4 CPUs (desktop profile) | release, fat LTO, stripped, glibc | 586 ms | 91.8 MiB (96.3 MB) | pass |
| GitHub runner, Linux aarch64, 4 CPUs (desktop profile) | same | 347 ms | 88.5 MiB (92.8 MB) | pass |
| Dev phone, Android, 2x A75 + 6x A55, 3.6 GiB, under proot (android profile) | release, fat LTO, stripped, bionic | 1244 ms | 74.8 MiB (78.4 MB) | pass |

Budget: 100 MB = 100,000,000 bytes = 95.4 MiB. All numbers come from `tendril bench` (D-028): an existing world created by a warm-up run, fixed seed, Anvil, the host's own profile, medians of 3 runs. CI rows are run 41386008433 at commit 4c1fd30a2; the phone row merges three single runs of the CI artifact from the same commit (sha256 `912181061df0f338...`). Raw JSON is in `bench/results/m2-2026-11-02/`.

Caveats:

- "Desktop" is a GitHub-hosted 4-CPU VM; no physical desktop was available.
- Section 4 times the first Java *and* Bedrock login. There is no Bedrock bot yet, so only Java is measured.
- The phone runs under proot (ptrace on every syscall), which makes it slower than native Termux. `~/tendril-m2 bench` in Termux gives the native number.
- On the phone, one of the three runs had 41 MiB swapped out by Android; the median and both clean runs (74.8 and 76.0 MiB) have none.

## What changed

| Change | Where | Effect (measured) |
| --- | --- | --- |
| `tendril bench`: startup, idle RSS/swap/CPU, 1/10/20 bots, MSPT from `/tick query`, budget verdicts, JSON | `launcher/setup/src/bench` | the instrument for everything below |
| Pause ticking when empty, like vanilla (`pause-when-empty-seconds`, 60 s) | `crates/pumpkin-core` (upstream PR draft in `docs/upstream/`) | idle CPU on the phone 8-17% to 0.8% of a core; desktop 0.7% to 0.1% |
| Cap glibc malloc arenas at 2-4 | `launcher/setup/src/memory.rs` | Linux aarch64 idle RSS 133-134 MiB to 88 MiB |
| Hand free memory back every 10 s while empty (`malloc_trim`, bionic `M_PURGE`) | same | Linux x86_64 idle RSS 107 MiB to 92 MiB (combined with the cap) |
| Release builds with fat LTO and stripped symbols | Bench workflow | Android binary 182 MB (no LTO) to 95 MB; idle RSS on the phone 96 to 75 MiB with the changes above |
| Kept the system allocator | D-025 | mimalloc raised idle RSS on every target (+104%, +81%, +7%) |
| Kept Anvil as the default world format | D-030 | Linear +5.6 MiB idle; Pump equal to Anvil; startup differences within noise |

## Measured detail

Idle RSS by malloc arena cap, Linux, desktop profile, median of 3 (two independent runs each):

| Cap | x86_64 | aarch64 |
| --- | --- | --- |
| glibc default (32 on 4 CPUs) | 93.4 / 93.8 MiB | 134.2 / 133.1 MiB |
| Tendril (profile workers, 2-4) | 92.1 / 91.8 MiB | 87.8 / 88.5 MiB |
| 2 | 90.3 / 91.6 MiB | 88.5 / 88.0 MiB |

Tick times stayed far below the 50 ms budget everywhere (p99 under 0.4 ms on the runners, under 25 ms on the phone) with 20 bots; the bots stand at spawn, so this is light load.

Memory per player (RSS + swap, 1 to 20 bots): x86_64 1.7 MiB, aarch64 1.7 MiB, phone 1.8 MiB per player.

By view distance (x86_64, desktop profile, run 41389187536 at 8f36e8943; RSS + swap in MiB):

| View distance | Idle | 1 bot | 10 bots | 20 bots | Per player |
| --- | --- | --- | --- | --- | --- |
| 6 | 90.5 | 92.4 | 104.6 | 123.4 | 1.63 |
| 8 | 90.9 | 93.4 | 105.4 | 126.2 | 1.73 |
| 10 | 91.3 | 94.3 | 106.1 | 125.6 | 1.65 |

View distance hardly matters here because every bot stands at the same spawn point and they share one loaded area. Players who spread out each load their own chunks, so these per-player figures are a lower bound; walking bots are the follow-up that turns them into a forecast.

Where the phone's 96 MiB went before the trim, from `/proc/<pid>/smaps` 30 s after a bot left: heap 42 MiB, binary read-only data 32 MiB (13 MiB of it private dirty from load-time relocations), binary code 17 MiB, stacks and the rest 4 MiB.

## Not done or partial

- Bedrock bot and Bedrock login time: none; needs a NetherNet client.
- The 0.2.1 to master join slowdown from M0 (+0.8 s to spawn on the phone) was not bisected; startup is inside budget everywhere, so it moved to the backlog.
- 13 MiB of private dirty memory on Android comes from relocating pointers in Pumpkin's generated static data (`.data.rel.ro`); Android requires position-independent executables, so only a codegen change upstream can remove it.
- Bots stand still, so MSPT under load is optimistic; walking bots that load new chunks are a follow-up.
- Pumpkin's simulation and compute thread pools still size from all CPUs (D-018).
- The nightly schedule and RSS/startup regression gates of section 14 are not active: they need the fork's default branch to be `main` and stored baselines.
