# Tiers

Tendril runs one of three engines. At startup it picks the lightest one that can run what is in the server folder and prints its choice, for example:

```text
Tier: Lite (no .jar files in plugins/ or mods/), see docs/tiers.md
```

Force a tier with `tendril --tier lite|bridge|compat` or `[tiers] mode` in `server.toml`.

| Tier | Picked when | What runs | Memory | Status |
| --- | --- | --- | --- | --- |
| **Lite** | no `.jar` files in `plugins/` or `mods/` | Pumpkin, in-process, no JVM. Pumpkin's own plugins (`.wasm`) load from `plugins/` | 77-90 MiB idle on a phone (see `docs/m0-baseline.md`) | works |
| **Bridge** | Paper/Bukkit `.jar` files in `plugins/` | Pumpkin plus an embedded JVM running PatchBukkit | Lite plus the JVM; measured when it exists | not built yet (M5) |
| **Compat** | Fabric `.jar` files in `mods/` | The official 26.3 server with Fabric Loader in a child JVM, behind Tendril's ports | vanilla-sized, likely around 1 GB; measured when it exists | not built yet (M6) |

Until a tier exists:

- `mode = "auto"` with jars present starts Lite anyway and warns that the jars will not load.
- Forcing `bridge` or `compat` refuses to start and says which milestone delivers the tier.

Switching tiers keeps the same world only in Anvil format (`format = "anvil"`, the default); the other tiers cannot open Linear or Pump worlds.
