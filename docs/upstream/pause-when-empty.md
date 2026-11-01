# Upstream PR draft: pause ticking when the server is empty

Branch: `upstream/pause-when-empty` on `VoidWalker1320/tendril` (one commit on top of Pumpkin `master` 2709e8b7a).
Open it yourself against `Pumpkin-MC/Pumpkin:master`; Pumpkin does not accept PRs opened by agents.

## Before you open it

1. **Play test it** with a real 26.3 client (no screenshot needed, nothing changes on screen):
   - Start the server, join, leave. About 60 s later the console prints `Server empty for 60 seconds, pausing`.
   - Join again: you spawn normally, time of day and weather move again, mobs move, and `/tick query` reports fresh tick times.
   - Stay online for more than 60 s: no pause message while anyone is online.
   - With `/tick sprint 1000` running and nobody online, the server does not pause (vanilla counts sprinting as busy).
2. Check open PRs once more for anything similar.
3. Keep the disclosure line where it is.

## Title

feat(server): pause ticking when the server is empty, like vanilla

## Body

```markdown
## Dеscription ·

> This description was drafted by an AI agent (Claude Code).

Vanilla's dedicated server stops ticking once nobody has been online for `pause-when-empty-seconds` (default 60). Pumpkin already keeps that setting in `ManagementHub` for the management protocol (`minecraft:serversettings/pause_when_empty_seconds`) but the tick loop never read it, so an empty server kept ticking all its worlds. On a phone that is 7-12% of a CPU core with nobody online.

This ports the check from `MinecraftServer.tickServer` (26.3):

- `emptyTicks` counts ticks with no players, reset by any player; while sprinting the server counts as busy. With the setting at 0 or below nothing changes, and the counter is left alone, as in vanilla.
- From `pause-when-empty-seconds * 20` ticks on, the whole tick is skipped before `tickRateManager.tick()`: no tick count increment, no tick-time sample, no world or player ticking. Pumpkin's connections run on their own tasks, so there is no counterpart to vanilla's `tickConnection()` to keep running.
- On the first skipped tick it logs `Server empty for {} seconds, pausing` and autosaves. `request_autosave` uses the same per-world path as the periodic autosave, so `/save-off` is respected; player data is already saved when each player leaves.

`advance_empty_ticks` holds the counting as a `const fn` next to `apply_overload_skip`, with tests for the threshold, the reset and the disabled case. The duplicated stop-check and wait at the end of the loop moved into `wait_unless_stopping` so the paused path can share it.

Behaviour changes worth knowing:

- `ServerTickStartEvent` and `ServerTickEndEvent` are not fired while paused, since no tick happens.
- The value can only be changed through the management protocol; `pumpkin.toml` has no key for it. A config key mirroring `server.properties` could follow separately.

## Tеsting ·

- `cargo clippy -p pumpkin-core --all-targets --all-features` and `cargo test -p pumpkin-core --lib server::ticker` (3 new tests) pass.
- Ran the server with nobody online on an aarch64 Android phone and sampled CPU every 10 s: 7-12% of a core before the pause, 0.9% after `Server empty for 60 seconds, pausing` appeared at 60 s.
- Bots joined a paused server, spawned and were ticked normally.
- Not verified: a real client joining a paused server; the play-test steps are below.
- No in-game visual change, so no capture.
```
