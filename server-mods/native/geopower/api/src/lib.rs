//! GeoPower: power (FE, NeoForge's energy unit) for Horizon, built like Mekanism: this core mod holds
//! the grid, the cables and the machines; addons depend on it (`[dependencies] geopower = "^0.2"` in
//! their horizon-mixin.toml) and use its public modules:
//!
//! - `geopower_generators`: generators that feed the grid ([`grid::Grid::route`], [`grid::Grid::send`]).
//! - `geopower_ironfarm`: a multiblock iron farm that takes power from its port ([`grid::Grid::book`]).
//!
//! The blocks, items, recipes and names of all three are the content mod `geopower`
//! (server-mods/content/geopower), which players install too.
use horizon_client::mod_api::{
    BlockEntityContainerEvent, BlockEntityEnergyEvent, BlockEntityEvent, BlockEntityTickEvent,
    BlockEntityTicksEvent, Compound, Flow, Hooks, Host, ModContainer, ModEnergyBuffer,
    RightClickBlockEvent, Text, UseItemOnEvent,
};

pub mod cables;
pub mod grid;
pub mod machines;
pub mod smelting;
pub mod world;

use cables::Tier;
use machines::Furnace;
use world::{Live, block_name, num};

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Core)
}

struct Core;

impl Hooks for Core {
    fn block_entity(
        &self,
        _host: &mut dyn Host,
        event: &mut BlockEntityEvent<'_>,
    ) -> Option<(String, Compound)> {
        Furnace::from_id(event.block).map(|f| (f.id().to_owned(), Compound::new()))
    }

    fn block_entity_ticks(&self, _host: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        if Furnace::from_id(event.block).is_some() {
            event.ticks = true;
        }
    }

    fn block_entity_energy(
        &self,
        _host: &dyn Host,
        event: &mut BlockEntityEnergyEvent<'_>,
    ) -> Option<ModEnergyBuffer> {
        Furnace::from_id(event.block).map(|_| Furnace::buffer())
    }

    fn block_entity_container(
        &self,
        _host: &dyn Host,
        event: &mut BlockEntityContainerEvent<'_>,
    ) -> Option<ModContainer> {
        Furnace::from_id(event.block).map(Furnace::container)
    }

    fn block_entity_tick(&self, host: &mut dyn Host, event: &mut BlockEntityTickEvent<'_>) {
        if let Some(furnace) = Furnace::from_id(&event.entity.id) {
            machines::tick(furnace, host, event.pos, event.entity);
        }
    }

    /// An alloy on a cable upgrades it, and every cable of its tier joined to it, one tier
    /// (Mekanism's alloy upgrade).
    fn use_item_on(&self, host: &mut dyn Host, event: &mut UseItemOnEvent<'_>) -> Flow {
        let Some(to) = Tier::upgraded_by(&event.stack.item) else {
            return Flow::Continue;
        };
        let world = Live(&*host);
        let Some(from) = cables::tier_at(&world, event.pos) else {
            return Flow::Continue;
        };
        let Some(player) = host.acting_player() else {
            return Flow::Continue;
        };
        if to.below() != Some(from) {
            let line = Text::translate(
                "geopower.cable.wrong_alloy",
                &[tier_name(from), tier_name(to.below().unwrap_or(Tier::Basic))],
            );
            host.action_bar(player.uuid, line);
            return Flow::Cancel;
        }
        let cables = cables::joined_of_tier(&Live(&*host), event.pos, from);
        let alloy = to.alloy().expect("an upgrade tier has an alloy");
        if host.take_items(alloy, 1) == 0 {
            return Flow::Continue;
        }
        for pos in &cables {
            host.set_block_property(*pos, "tier", to.name());
        }
        let line = Text::translate(
            "geopower.cable.upgraded",
            &[num(cables.len()), tier_name(to), num(to.throughput())],
        );
        host.action_bar(player.uuid, line);
        Flow::Cancel
    }

    fn right_click_block(&self, host: &mut dyn Host, event: &mut RightClickBlockEvent) -> Flow {
        let block = host.block(event.pos);
        let line = if let Some(furnace) = Furnace::from_id(&block) {
            let node = grid::lock()
                .node((host.dimension(), event.pos))
                .unwrap_or_default();
            Text::translate(
                "geopower.furnace.status",
                &[
                    block_name(furnace.id()),
                    num(node.energy),
                    num(machines::CAPACITY),
                    num(furnace.usage()),
                ],
            )
        } else if let Some(tier) = cables::tier_at(&Live(&*host), event.pos) {
            Text::translate(
                "geopower.cable.status",
                &[tier_name(tier), num(tier.throughput())],
            )
        } else {
            return Flow::Continue;
        };
        if let Some(player) = host.acting_player() {
            host.action_bar(player.uuid, line);
        }
        Flow::Continue
    }
}

/// A cable tier's translated name.
pub fn tier_name(tier: Tier) -> Text {
    Text::translate(&format!("geopower.tier.{}", tier.name()), &[])
}
