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
- **Dependencies between trait mods** work as a NeoForge addon's `[[dependencies.<mod>]]` in `neoforge.mods.toml`
  (MekanismGenerators on Mekanism): `[dependencies]` in the dependent's `horizon-mixin.toml`, with Cargo's semver
  requirements against the other bundle's `[mod] version`:

  ```toml
  [dependencies]
  geopower = "^0.2"                                   # required
  other_mod = { version = "^1", optional = true }     # used when installed
  ```

  The build stops with the mods named when a required one is missing, a version doesn't fit or they depend on each
  other in a cycle; it links the mods in dependency order and gives the dependent's package a dependency named by
  the mod id (`geopower::grid` in Rust). An optional one also gets a Cargo feature of that name, on only when the mod
  is installed (`#[cfg(feature = "other_mod")]`). Don't write these into the package's `Cargo.toml`.
- **Content mods** (blocks, items, recipes, loot...) are copied to `/data/horizon-mods/` and turned on at every
  start. A mod with `side = "both"` must be installed in the same version on every player's Horizon too, or the
  server refuses the join and names the mod.

Players join with the Horizon client (Multiplayer > the server's address). Native server-side mods need nothing
on the players' side; their content mods do.

## GeoPower

Built like Mekanism: a core mod and addons that depend on it.

| Native mod | Adds | Depends on |
|---|---|---|
| `native/geopower` | the power grid, universal cables (4 tiers), alloys, electric furnace, blast furnace, smoker | — |
| `native/geopower_generators` | solar panel, wind turbine, hydro and geothermal generators | `geopower ^0.2` |
| `native/geopower_ironfarm` | the iron farm multiblock and its power port | `geopower ^0.2` |

All their blocks, items, models (vanilla textures), recipes, loot and names (English, German) are one content mod,
`content/geopower`. **Players install it**: put `geopower.hmod` (built into the image; copy it from the server's
`/data/horizon-mods/`, or build it with `cargo horizon-mod build` in that folder) into their game folder's
`horizon-mods/` and turn it on in Options > Mods. The rest runs on the server.

Right-click any GeoPower block to see what it holds, makes or lacks. **The guide** is a written book: craft a book
with a redstone dust. It explains everything below and how to build the iron farm, layer by layer.

### Generators

| Block | Makes power from | Peak |
|---|---|---|
| Solar panel | daylight on open sky; none at night, less in rain | 20 FE/t at noon |
| Wind turbine | height above sea level (y 63) and open air in the 3×3 around it; storms double it | 32 FE/t |
| Hydro generator | water on any side: source 1, flowing 4, falling or bubble column 6 FE/t | 36 FE/t |
| Geothermal generator | lava (source 8, flowing 4), lava cauldron 6, magma 2, fire 1 FE/t per side; ×1.5 below y 0, ×1.25 in the Nether | 72 FE/t |

Each holds 100 000 FE (NeoForge's energy unit) and sends up to 1 000 FE a tick to the nearest receivers along the
cables (or touching it); other mods may take it out of any side (`Capabilities.Energy.BLOCK`).

### Cables

One block, `geopower:cable` (8 from iron, redstone, iron), in Mekanism's four tiers. The weakest cable between a
generator and a machine sets how much goes through: Basic 200, Advanced 800, Elite 3 200, Ultimate 12 800 FE/t
(Mekanism's own start at 8 000 FE/t; these fit generators that make tens). **Upgrade** as in Mekanism: right-click a
cable with an alloy; it and up to 64 joined cables of the same tier go up one tier, for one alloy. Breaking an
upgraded cable gives the cable and its alloys back.

| Alloy | Recipe (shapeless; Mekanism infuses these) | Upgrades |
|---|---|---|
| Infused | copper ingot, 2 redstone | Basic → Advanced |
| Reinforced | infused alloy, diamond | Advanced → Elite |
| Atomic | reinforced alloy, obsidian, ender pearl | Elite → Ultimate |

### Electric furnaces

Vanilla's furnace screen, no fuel: the flame shows the charge, the arrow the progress. Hoppers fill the input from
above or the sides and take the output from below.

| Block | Cooks | Speed | Power |
|---|---|---|---|
| Electric furnace | everything a furnace does (the common recipes) | 2× (100 ticks) | 20 FE/t |
| Electric blast furnace | ores, raw metal, ancient debris | 4× (50 ticks) | 40 FE/t |
| Electric smoker | food | 4× (50 ticks) | 20 FE/t |

### Iron farm (multiblock)

Built as a classic iron farm, 3 × 3 wide and 4 high, facing any way (the front row holds the controller):

```
Layer 1, floor         Layer 2, lava blade    Layer 3, village pod   Layer 4, roof
  S C S                  G G G                  H G H                  B B B      front
  S T S                  G L G                  G K G                  B Z B
  S P S                  G G G                  H G H                  B B B      back
```

S smooth stone · C iron farm controller · T hopper · P power port · G glass · L lava source · H oak log ·
K composter · B oak planks · Z zombie head.

With 40 FE/t at the power port it makes an iron ingot every 15 s (and a poppy with every fourth) into the
controller's 9 slots; a hopper under the controller empties them. A right-click on the controller names the first
missing block (layer, row from the front, column) or what it needs.

The logic and its tests: `native/geopower/api/src/` (`grid.rs`, `cables.rs`, `machines.rs`, `smelting.rs`),
`native/geopower_generators/api/src/generators.rs`, `native/geopower_ironfarm/api/src/structure.rs`.
