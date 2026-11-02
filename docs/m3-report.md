# M3 report: Bedrock pass (2026-11-02)

Gate: the latest Bedrock client joins, builds, chats, uses containers and fights; Floodgate-style UUIDs.

Status: everything a bot can check passes. The real-client half of the gate (build, containers, fight, rejoin) waits on a Bedrock client: `docs/m3-bedrock-checklist.md`.

## Done

| Change | Where | Check |
| --- | --- | --- |
| Merged Pumpkin `master` 83efbd3a1 into `m3` (D-036) | merge commit | CI on all targets |
| Floodgate UUIDs: `UUID(0, xuid)`, same as Floodgate's `Utils.getJavaUuid`; Pumpkin option `floodgate_uuids`, on in Tendril | `pumpkin-config`, `pumpkin-core` login, `tendril-setup` settings | 2 unit tests (known XUID to UUID, fallback) |
| XUIDs are trusted only from Xbox-verified logins; offline and self-signed logins keep the token UUID (D-032) | `authenticate()` in Bedrock login | 3 unverified BotMark bots join as 3 separate players; no test of a verified login yet |
| Account linking: `/linkaccount <bedrock name>` on Java gives a 6-digit code, `/linkaccount <java name> <code>` on Bedrock links, `/unlinkaccount` (D-033) | `tendril_setup::linking` (store), `launcher/src/linking.rs` (commands), `Server::bedrock_account_links` (hook) | 5 store tests + 1 launcher test; commands answer on the phone |
| OIDC key fetch retries (5 s doubling to 5 min) instead of storing an empty key set for good | `pumpkin-core` server | clippy; original failure seen on the phone |
| Bedrock bots in CI: BotMark over NetherNet, pinned and patched (D-035) | `bench/bedrock_bots_test.py`, CI job | see Measured |
| Defaults from section 8 (already in M1): `online_mode = true`, prefix `.`, spaces to `_` | `tendril-setup` settings | bots chat as `.BOT_1` (run 41427145875) |
| Supported range as one constant: `CURRENT_BEDROCK_MC_PROTOCOL` (2225) | `pumpkin-world` | CI reads BotMark's version from it |
| Upstream PR drafts: OIDC retry, Floodgate UUIDs (D-037) | `docs/upstream/`, branches `upstream/bedrock-*` | fmt, clippy, tests on Pumpkin `master` + both branches: run 41462476764 |

## Measured

- **CI run 41459739041 at 3e5aeef1b:** 10 of 11 jobs passed: launcher, Pumpkin crates, first run on Linux x86_64/aarch64 (glibc and musl) and Windows, macOS x86_64 build, Android build, and Bedrock bots. GitHub never assigned a runner for macOS arm64, twice ("not acquired ... capacity constraints", while githubstatus.com showed Actions operational). It last passed at M2 (`main` 8c3dbc76a); the run for the docs commits on top tries again. macOS x86_64, which builds the same launcher code for macOS, passed.
- **Bedrock bots** (ubuntu-latest x86_64, 3 BotMark bots for 45 s, protocol 2225): `{"bots": 3, "spawned": 3, "chat_lines_on_server": 3, "panics": 0, "packet_errors": 0}`. Bots join over NetherNet, spawn into the world and their chat reaches the server. The test does not check movement.
- **Phone** (Android, under proot, CI artifact without LTO, sha256 `c592d84eb0b3d802...`, single run): `Ready in 968 ms, RSS 67.0 MiB`. A Java bot joined, and console `/linkaccount` and `/unlinkaccount` answered "Only players can ...". This is the server's own log line, not a `tendril bench` median.
- Not measured: Bedrock login time and memory per Bedrock player. `tendril bench` has no Bedrock bot, and BotMark is not built on the phone (D-035).

## Not done or partial

- **Real-client gate:** build, containers, crafting, combat, rejoin with the same data, skins, autocomplete, riding and resource packs need a real Bedrock client: `docs/m3-bedrock-checklist.md`. Bots only cover login, spawning and chat. BotMark's arm swing (`Animate`) predates 26.60 and is off.
- **Previous minor line** (26.50-26.52, protocol 2193): deferred (D-034). It needs version-aware encoding for seven packets plus 26.5x block and item tables, which is a Bedrock translation layer like M4's.
- **Xbox-verified paths** (Floodgate UUID applied to a real XUID, `/linkaccount` confirm, OIDC retry recovering) compile and pass clippy; the UUID derivation and the link store have unit tests, the retry loop has none. None has run end to end: the bots join an offline-mode server, which never verifies an XUID. The real-client checklist covers the first two.
- **Bedrock forms:** Pumpkin already lets Wasm plugins send `CModalFormRequest` and receive `BedrockFormResponseEvent`. No plugin exercises this yet, so it is unverified. PatchBukkit (M5) is where Paper plugins would reach it.
- **Global (GeyserMC-hosted) linking:** left out on purpose (D-033).
- **BotMark fixes** (per-bot XUID, `MaxViewDistance`, 26.60 `Animate`) live as CI patches, not as a BotMark PR.

## Version check (2026-11-02)

| Component | Found | Source |
| --- | --- | --- |
| Bedrock latest release | 1.26.61, protocol 2225 (same as 1.26.60, released 27 Oct) | Mojang `bedrock-protocol-docs` commit 21e84e70f (29 Oct 2026), `bedrock-samples` v1.26.60.5 |
| Bedrock previous minor line | 26.50, 26.51, 26.52 on protocol 2193 | Geyser `master` b9d535a67 `GameProtocol.java` |
| Bedrock next | 1.26.70 in preview, protocol 2239 | `bedrock-protocol-docs` |
| Floodgate UUID scheme | `new UUID(0, xuid)` | Floodgate `master` 81b65ccb7 `Utils.getJavaUuid` |
| Pumpkin `master` | 83efbd3a1, merged | `upstream/master` |
| Java | 26.3 is still the latest release (26.4 snapshot 6 is the newest snapshot), protocol 777 | Mojang `version_manifest_v2.json` |

## Decisions

D-032 Floodgate UUIDs only for Xbox-verified XUIDs · D-033 local account linking with codes · D-034 Bedrock 26.60/26.61 only, 26.5x deferred · D-035 BotMark in CI, pinned and patched · D-036 upstream merged, not rebased · D-037 what goes upstream (OIDC retry, Floodgate UUIDs), what stays (link hook).

## Next

- You run `docs/m3-bedrock-checklist.md` with a Bedrock 26.60/26.61 client and report each row. Fixes for anything that fails are part of M3.
- After that passes, fast-forward `main` to `m3`.
- You open the two upstream PRs from `docs/upstream/` after their play tests. Agents may not open them.
- M4 (Java 1.20 to 26.3 translation layer) starts on "go". The deferred Bedrock 26.5x support sits next to it in the backlog.
