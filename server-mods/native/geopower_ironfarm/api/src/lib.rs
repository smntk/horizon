//! GeoPower Iron Farm, an addon of the core mod `geopower`: a multiblock built like a classic iron
//! farm (`structure`) that turns power at its power port into iron ingots, with a poppy now and then
//! as an iron golem drops. Cables bring the power to the port; the ingots wait in the controller's
//! nine slots, which a hopper under it empties.
use geopower::grid;
use geopower::world::{Live, block_name, int_of, num, store};
use horizon_client::mod_api::{
    BlockEntityContainerEvent, BlockEntityEnergyEvent, BlockEntityEvent, BlockEntityTickEvent,
    BlockEntityTicksEvent, Compound, Flow, Hooks, Host, ItemStack, ModBlockEntity, ModContainer,
    ModEnergyBuffer, RightClickBlockEvent, Text,
};
use std::collections::BTreeMap;
use std::sync::Mutex;

pub mod structure;
use structure::{CONTROLLER, Issue, PORT};

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(IronFarm)
}

/// FE per tick the farm takes from its port while it works.
pub const USAGE: i32 = 40;
/// Ticks per ingot: one every 15 seconds, about what a small vanilla iron farm makes.
pub const CYCLE: i32 = 300;
/// Every this many ingots a poppy comes with it.
pub const POPPY_EVERY: i32 = 4;
/// FE the power port holds.
pub const PORT_CAPACITY: i32 = 50_000;
const SLOTS: usize = 9;
/// How often (in ticks) a controller checks its farm again.
const CHECK_TICKS: u32 = 20;

/// Each controller's last check of its farm and the ticks to its next.
static FARMS: Mutex<BTreeMap<grid::Key, (Result<[i32; 3], Issue>, u32)>> =
    Mutex::new(BTreeMap::new());

struct IronFarm;

fn ours(block: &str) -> bool {
    block == CONTROLLER || block == PORT
}

impl Hooks for IronFarm {
    fn block_entity(
        &self,
        _host: &mut dyn Host,
        event: &mut BlockEntityEvent<'_>,
    ) -> Option<(String, Compound)> {
        ours(event.block).then(|| (event.block.to_owned(), Compound::new()))
    }

    fn block_entity_ticks(&self, _host: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        if ours(event.block) {
            event.ticks = true;
        }
    }

    fn block_entity_energy(
        &self,
        _host: &dyn Host,
        event: &mut BlockEntityEnergyEvent<'_>,
    ) -> Option<ModEnergyBuffer> {
        (event.block == PORT).then(|| ModEnergyBuffer {
            capacity: PORT_CAPACITY,
            max_insert: 1_000,
            max_extract: 0,
            insert_faces: [true; 6],
            extract_faces: [false; 6],
        })
    }

    fn block_entity_container(
        &self,
        _host: &dyn Host,
        event: &mut BlockEntityContainerEvent<'_>,
    ) -> Option<ModContainer> {
        (event.block == CONTROLLER).then(|| {
            let all: Vec<usize> = (0..SLOTS).collect();
            ModContainer {
                slots: SLOTS,
                faces: std::array::from_fn(|_| all.clone()),
                // Nothing goes in from hoppers; everything may be taken out.
                inputs: Vec::new(),
                outputs: all.clone(),
                menu: Some("minecraft:generic_3x3".to_owned()),
            }
        })
    }

    fn block_entity_tick(&self, host: &mut dyn Host, event: &mut BlockEntityTickEvent<'_>) {
        let key = (host.dimension(), event.pos);
        if event.entity.id == PORT {
            let mut grid = grid::lock();
            let energy = grid.settle(key, int_of(&event.entity.nbt, "energy"), PORT_CAPACITY);
            grid.publish(key, energy, PORT_CAPACITY, true);
            drop(grid);
            store(event.entity, "energy", energy);
        } else if event.entity.id == CONTROLLER {
            tick_controller(host, key, event.entity);
        }
    }

    fn right_click_block(&self, host: &mut dyn Host, event: &mut RightClickBlockEvent) -> Flow {
        let block = host.block(event.pos);
        if !ours(&block) {
            return Flow::Continue;
        }
        let key = (host.dimension(), event.pos);
        let line = if block == PORT {
            let energy = grid::lock().node(key).unwrap_or_default().energy;
            Text::translate(
                "geopower.port.status",
                &[num(energy), num(PORT_CAPACITY)],
            )
        } else {
            match structure::check(&Live(&*host), event.pos) {
                Err(issue) => Text::translate(
                    "geopower.iron_farm.missing",
                    &[
                        num(issue.layer),
                        num(issue.row),
                        num(issue.column),
                        block_name(issue.expected),
                    ],
                ),
                Ok(port) => {
                    let energy = grid::lock().node((key.0, port)).unwrap_or_default().energy;
                    if energy < USAGE {
                        Text::translate("geopower.iron_farm.no_power", &[num(USAGE)])
                    } else {
                        Text::translate(
                            "geopower.iron_farm.running",
                            &[num(energy), num(USAGE), num(CYCLE / 20)],
                        )
                    }
                }
            }
        };
        if let Some(player) = host.acting_player() {
            host.action_bar(player.uuid, line);
        }
        // The controller still opens: its slots hold the ingots.
        Flow::Continue
    }
}

/// One tick of a controller: checks its farm now and then, takes power from the port while there is
/// room for what it makes, and puts an ingot (and every fourth time a poppy) into its slots.
fn tick_controller(host: &mut dyn Host, key: grid::Key, entity: &mut ModBlockEntity) {
    if entity.items.len() < SLOTS {
        return;
    }
    let farm = {
        let mut farms = FARMS.lock().unwrap_or_else(|e| e.into_inner());
        let entry = farms.entry(key).or_insert((Err(Issue::default()), 0));
        if entry.1 == 0 {
            *entry = (structure::check(&Live(&*host), key.1), CHECK_TICKS);
        }
        entry.1 -= 1;
        entry.0
    };
    let Ok(port) = farm else {
        return;
    };
    let ingot = stack("minecraft:iron_ingot");
    if !fits(&entity.items, &ingot) {
        return;
    }
    let port = (key.0, port);
    let mut grid = grid::lock();
    if grid.available(port) < USAGE as i64 {
        return;
    }
    grid.book(port, -(USAGE as i64));
    drop(grid);
    let progress = int_of(&entity.nbt, "progress") + 1;
    if progress < CYCLE {
        store(entity, "progress", progress);
        return;
    }
    store(entity, "progress", 0);
    let made = int_of(&entity.nbt, "made") + 1;
    store(entity, "made", made);
    let mut items = entity.items.clone();
    put(&mut items, ingot);
    if made % POPPY_EVERY == 0 {
        // A golem's poppy; left out when there is no room for it, so it never stops the farm.
        put(&mut items, stack("minecraft:poppy"));
    }
    entity.items = items;
}

fn stack(item: &str) -> ItemStack {
    ItemStack {
        item: item.into(),
        count: 1,
        damage: 0,
        extra: None,
    }
}

/// Whether one of `one` fits in `slots`.
fn fits(slots: &[Option<ItemStack>], one: &ItemStack) -> bool {
    slots.iter().any(|slot| match slot {
        None => true,
        Some(s) => s.item == one.item && s.extra.is_none() && s.count < 64,
    })
}

/// Puts one of `one` onto a matching stack, else into the first empty slot; false without room.
fn put(slots: &mut [Option<ItemStack>], one: ItemStack) -> bool {
    if let Some(Some(s)) = slots
        .iter_mut()
        .find(|slot| matches!(slot, Some(s) if s.item == one.item && s.extra.is_none() && s.count < 64))
    {
        s.count += 1;
        return true;
    }
    if let Some(slot) = slots.iter_mut().find(|slot| slot.is_none()) {
        *slot = Some(one);
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_stacks_first_and_reports_a_full_controller() {
        let mut slots: Vec<Option<ItemStack>> = vec![None; SLOTS];
        assert!(put(&mut slots, stack("minecraft:iron_ingot")));
        assert!(put(&mut slots, stack("minecraft:iron_ingot")));
        assert_eq!(slots[0].as_ref().map(|s| s.count), Some(2));
        assert!(slots[1].is_none());
        let full = ItemStack {
            count: 64,
            ..stack("minecraft:iron_ingot")
        };
        let mut slots = vec![Some(full); SLOTS];
        assert!(!fits(&slots, &stack("minecraft:iron_ingot")));
        assert!(!put(&mut slots, stack("minecraft:poppy")));
    }
}
