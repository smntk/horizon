//! GeoPower Generators, an addon of the core mod `geopower` (as MekanismGenerators is of Mekanism):
//! generators that make power from the world around them and send it along the core's cables.
//!
//! - Solar panel: sunlight on open sky (the "UV" one): strongest at noon, weaker in rain.
//! - Wind turbine: height above sea level and open air around it; storms blow harder.
//! - Hydro generator: flowing and falling water beside it.
//! - Geothermal generator: lava or magma touching it; deeper (below y 0) and the Nether are hotter.
use geopower::grid;
use geopower::world::{Live, block_name, int_of, num, store};
use horizon_client::mod_api::{
    BlockEntityEnergyEvent, BlockEntityEvent, BlockEntityTickEvent, BlockEntityTicksEvent,
    Compound, Flow, Hooks, Host, ModEnergyBuffer, RightClickBlockEvent, Text,
};
use std::collections::BTreeMap;
use std::sync::Mutex;

mod generators;
pub use generators::Generator;

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Generators)
}

/// What one generator holds, in FE.
pub const CAPACITY: i32 = 100_000;
/// The most a generator sends in one tick (and another mod takes out in one call).
pub const MAX_EXTRACT: i32 = 1_000;
/// How often (in ticks) a generator looks at its surroundings again; it makes the same power in between.
const SURVEY_TICKS: u32 = 20;

/// Each generator's power per tick from its last survey, and the ticks to its next.
static RATES: Mutex<BTreeMap<grid::Key, (i32, u32)>> = Mutex::new(BTreeMap::new());

struct Generators;

impl Hooks for Generators {
    fn block_entity(
        &self,
        _host: &mut dyn Host,
        event: &mut BlockEntityEvent<'_>,
    ) -> Option<(String, Compound)> {
        Generator::from_id(event.block).map(|_| (event.block.to_owned(), Compound::new()))
    }

    fn block_entity_ticks(&self, _host: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        if Generator::from_id(event.block).is_some() {
            event.ticks = true;
        }
    }

    fn block_entity_energy(
        &self,
        _host: &dyn Host,
        event: &mut BlockEntityEnergyEvent<'_>,
    ) -> Option<ModEnergyBuffer> {
        Generator::from_id(event.block).map(|_| ModEnergyBuffer {
            capacity: CAPACITY,
            // A generator only gives: nothing goes in, everything may come out of every side.
            max_insert: 0,
            max_extract: MAX_EXTRACT,
            insert_faces: [false; 6],
            extract_faces: [true; 6],
        })
    }

    /// Makes power, then sends what it can along the cables to the nearest receivers.
    fn block_entity_tick(&self, host: &mut dyn Host, event: &mut BlockEntityTickEvent<'_>) {
        let Some(generator) = Generator::from_id(&event.entity.id) else {
            return;
        };
        let world = Live(&*host);
        let key = (host.dimension(), event.pos);
        let rate = {
            let mut rates = RATES.lock().unwrap_or_else(|e| e.into_inner());
            let entry = rates.entry(key).or_insert((0, 0));
            if entry.1 == 0 {
                *entry = (generator.rate(&world, event.pos), SURVEY_TICKS);
            }
            entry.1 -= 1;
            entry.0
        };
        let mut grid = grid::lock();
        let held = grid.settle(key, int_of(&event.entity.nbt, "energy"), CAPACITY);
        let mut energy = held.saturating_add(rate).min(CAPACITY);
        let route = grid.route(key, &world);
        energy -= grid.send(key, &route, energy.min(MAX_EXTRACT) as i64) as i32;
        grid.publish(key, energy, CAPACITY, false);
        drop(grid);
        store(event.entity, "energy", energy);
    }

    fn right_click_block(&self, host: &mut dyn Host, event: &mut RightClickBlockEvent) -> Flow {
        let block = host.block(event.pos);
        let key = (host.dimension(), event.pos);
        let Some(generator) = Generator::from_id(&block) else {
            return Flow::Continue;
        };
        let Some(player) = host.acting_player() else {
            return Flow::Continue;
        };
        let rate = RATES
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .map_or(0, |r| r.0);
        let energy = grid::lock().node(key).unwrap_or_default().energy;
        let line = if rate > 0 {
            Text::translate(
                "geopower.status",
                &[block_name(&block), num(energy), num(CAPACITY), num(rate)],
            )
        } else {
            let hint = Text::translate(&format!("geopower.hint.{}", generator.path()), &[]);
            Text::translate(
                "geopower.status.idle",
                &[block_name(&block), num(energy), num(CAPACITY), hint],
            )
        };
        host.action_bar(player.uuid, line);
        // The click goes on: a block in hand is still placed against the generator.
        Flow::Continue
    }
}
