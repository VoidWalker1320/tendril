# Upstream PR draft: optional Floodgate-style Bedrock UUIDs

Branch: `upstream/bedrock-floodgate-uuids` on `VoidWalker1320/tendril` (one commit on top of Pumpkin `master` 83efbd3a1).
Open it yourself against `Pumpkin-MC/Pumpkin:master`; Pumpkin does not accept PRs opened by agents.

This is the part of Tendril's M3 change that makes sense for Pumpkin on its own. Tendril's account-link hook (`Server::bedrock_account_links`) stays in the fork: it only exists for Tendril's `/linkaccount`, and AGENTS.md asks for no hooks without an in-tree user.

## Before you open it

1. **Play test it** with an Xbox-signed Bedrock client. No in-game visual change (the player-data file name is the check), so no capture is needed:
   - In `pumpkin.toml`, under `[networking.bedrock]`, set `floodgate_uuids = true`. Join, then leave.
   - `world/players/data/` has a file named `00000000-0000-0000-xxxx-xxxxxxxxxxxx.dat`, where the last 16 hex digits are your XUID in hex. A Geyser + Floodgate server shows the same UUID for you (`/list uuids` on Paper, or its player data folder).
   - With the option off (default), you get the same UUID as on `master`.
   - Then replace the "Not verified yet" line in the body with what you saw.
2. Check open PRs once more for anything similar (none on 2026-11-02 for "floodgate", "xuid" or "bedrock uuid").
3. Keep the disclosure line where it is.

## Title

feat(bedrock): optional Floodgate-style player UUIDs from verified XUIDs

## Body

```markdown
## Dеscription ·

> This description was drafted by an AI agent (Claude Code).

Servers that move from Geyser + Floodgate to Pumpkin lose every Bedrock player's data, because Pumpkin uses the identity UUID from the login token while Floodgate uses `new UUID(0, xuid)` (`Utils.getJavaUuid` in Floodgate). The new `networking.bedrock.floodgate_uuids` option (off by default, so existing worlds keep their players) gives Bedrock players the Floodgate UUID instead.

Only a fully authenticated Xbox login proves its XUID. A self-signed login (allowed when `authentication.enabled` is off) or an offline-mode login can put any XUID in its token, so with the option on it could take over another player's UUID. Those logins keep the token UUID. To know which case it is, the token checks moved out of `try_handle_login` into `authenticate()`, which returns the claims plus whether the XUID is proven. The checks themselves did not change.

## Tеsting ·

- `cargo fmt --check`, `cargo clippy -p pumpkin-core -p pumpkin-config --all-targets --all-features`, `cargo test -p pumpkin-core --lib net::bedrock::login` (9 passed) and `cargo test -p pumpkin-config` pass on this branch (fork CI run 41462476764, which checks it merged with my other open Bedrock branch on master 83efbd3a1).
- Two tests: the UUID for XUID 2535419381938497 is `00000000-0000-0000-0009-01f3539df941`, and the token UUID is kept when the option is off or the XUID is empty.
- Bedrock bots (BotMark over NetherNet against an offline-mode server) still join, spawn and chat with the option on. Their logins are unverified, so the option does not apply to them; the Xbox path is the one the human test below covers.
- Not verified yet: an Xbox-signed client with the option on. Steps for the human tester: set `floodgate_uuids = true`, join, check `world/players/data/` holds `00000000-0000-0000-<XUID in hex>.dat`.
```
