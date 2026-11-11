# Server mods

The mods the Coolify server (`docker-compose.yml`, `packaging/coolify/`) is built with. Commit a mod here and push:
Coolify rebuilds the image with it and restarts the server.

```
server-mods/
  native/<mod>/horizon-mixin.toml     native Rust mods (docs/native-mods.md): compiled into the server's game
  native/<mod>/api/                   ...a trait mod's package: implements horizon_client::mod_api::Hooks
  content/<mod>/horizon-mod.toml      content mods from source (docs/mods.md): built into an .hmod
  content/<name>.hmod                 content mods already built
```

- **Native mods** are Rust compiled into the server's game. They run unsandboxed on the server: only add code you
  have read. A bundle can be
  - a **trait mod**: `[api] path = "api"` names a Rust package whose `pub fn hooks() -> Box<dyn Hooks>` implements
    the hook points it needs (`horizon_client::mod_api::Hooks`, docs/native-mods.md "Trait mods"). The build copies
    the package into the workspace, points its `horizon-client` dependency at this source and links it into a
    generated `horizon-native-game` executable, as the launcher's `native-build` does. Start one with
    `cargo horizon-mod new <path> --template native`. Its other dependencies must be crates already in Horizon's
    `Cargo.lock`.
  - a **mixin bundle**: `[[injection]]`s, modules and crates applied with `horizon-mixins apply`. Trait injections
    (`trait_method`) and post-build steps need the launcher's build (`horizon-launcher native-build --server-output`).
- **Content mods** (blocks, items, recipes, loot...) are copied to `/data/horizon-mods/` and turned on at every
  start. A mod with `side = "both"` must be installed in the same version on every player's Horizon too, or the
  server refuses the join and names the mod.

Players join with the Horizon client (Multiplayer > the server's address). Native server-side mods need nothing
on the players' side; their content mods do.

## GeoPower (power generators)

| Block | Makes power from | Peak |
|---|---|---|
| Solar panel | daylight on open sky; none at night, less in rain | 20 FE/t at noon |
| Wind turbine | height above sea level (y 63) and open air in the 3×3 around it; storms double it | 32 FE/t |
| Hydro generator | water on any side: source 1, flowing 4, falling or bubble column 6 FE/t | 36 FE/t |
| Geothermal generator | lava (source 8, flowing 4), lava cauldron 6, magma 2, fire 1 FE/t per side; ×1.5 below y 0, ×1.25 in the Nether | 72 FE/t |

Each holds 100 000 FE (NeoForge's energy unit, saved with the block) and gives up to 1 000 FE per call from any
side to another mod's machine or cable (`Capabilities.Energy.BLOCK`). Right-click one to see what it holds and
makes, or why it stands still.

- `content/geopower`: the blocks, models (vanilla textures), recipes, loot, names (English, German).
  **Players install it**: put the `geopower.hmod` (built into the image; copy it from the server's
  `/data/horizon-mods/`, or build it with `cargo horizon-mod build` in that folder) into their game folder's
  `horizon-mods/` and turn it on in Options > Mods.
- `native/geopower`: the power, a trait mod (server only). Its logic is in `api/src/generators.rs`, with tests.
