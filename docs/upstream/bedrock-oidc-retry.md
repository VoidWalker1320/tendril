# Upstream PR draft: retry the Bedrock OIDC key fetch

Branch: `upstream/bedrock-oidc-retry` on `VoidWalker1320/tendril` (one commit on top of Pumpkin `master` 83efbd3a1).
Open it yourself against `Pumpkin-MC/Pumpkin:master`; Pumpkin does not accept PRs opened by agents.

## Before you open it

1. **Check it once with a real client.** No in-game visual change, so no capture is needed:
   - Turn networking off (airplane mode, or block `client.discovery.minecraft-services.net`) and start the server with Bedrock `online_mode` and `authentication.enabled` on (the defaults). The console prints `Failed to fetch Bedrock OIDC keys, retrying in 5s: ...`, then 10s, 20s.
   - Turn networking back on. The messages stop within one retry interval.
   - Join with an Xbox-signed Bedrock client: you get in. On `master`, the same steps keep refusing every Xbox login until a restart.
   - Then replace the "Not verified yet" line in the body with what you saw.
2. Check open PRs once more for anything similar (none on 2026-11-02 for "OIDC", "xuid" or "bedrock auth").
3. Keep the disclosure line where it is.

## Title

fix(bedrock): keep retrying the OIDC key fetch instead of giving up

## Body

```markdown
## Dеscription ·

> This description was drafted by an AI agent (Claude Code).

`Server::new` fetches the Bedrock OIDC keys once. When that fetch fails, it stores an empty issuer and key set in `bedrock_oidc_keys`, and since that is a `OnceCell`, the server checks every later Xbox login against no keys until it restarts. On an Android phone a cold DNS lookup took about 5.6 s against the 5000 ms `connect_timeout`, so the fetch failed on the first start after boot and stayed failed.

Now a failed fetch leaves the cell unset and tries again after 5 s, doubling up to 5 minutes, until it succeeds or the server stops (`stop_token`). Each failure logs one line with the next delay.

Other readers of `bedrock_oidc_keys`, while the keys are not there yet (this window already existed on `master` between startup and the first fetch; it just lasts longer when the fetch fails):

- `verify_oidc_token_path` (Bedrock login): refuses Xbox logins with "OIDC keys not initialized" until the keys arrive, then accepts them. Before, they were refused for good.
- NetherNet offers on direct IP with `require_client_identity`: refused with "Bedrock authentication keys are not ready", then accepted. Before, they were checked against the empty key set and refused for good.
- NetherNet offers over LAN or HTTP signalling: their identity token only gets the expiry check until the keys arrive, as when authentication is off. The login packet itself is still verified as above. Before, any offer carrying an identity was refused for good.

## Tеsting ·

- `cargo fmt --check` and `cargo clippy -p pumpkin-core -p pumpkin-config --all-targets --all-features` pass on this branch (fork CI run 41462476764, which checks this branch merged with my other open Bedrock branch on master 83efbd3a1).
- Saw the original failure on the phone: `Failed to fetch Bedrock OIDC keys: Public key build failed: error sending request for url (https://client.discovery.minecraft-services.net/...)` at startup. From the code, every Xbox login after that is checked against the empty key set and refused; no Xbox client was tried at the time.
- No unit test: the change is a retry loop around a network call, and the useful check is the offline-then-online run in the steps below.
- Not verified yet: an Xbox login after the keys arrive on a retry. Steps for the human tester: start with networking off, turn it on, check the retry messages stop and an Xbox-signed client joins.
```
