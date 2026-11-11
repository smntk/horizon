//! GeoPower: generators that make power (FE, NeoForge's energy unit) from the world around them.
//!
//! - Solar panel: sunlight on open sky (the "UV" one): strongest at noon, weaker in rain.
//! - Wind turbine: height above sea level and open air around it; storms blow harder.
//! - Hydro generator: flowing and falling water beside it.
//! - Geothermal generator: lava or magma touching it; deeper (below y 0) and the Nether are hotter.
//!
//! The blocks are declared by the content mod `geopower` (server-mods/content/geopower). This trait
//! mod gives each of them a block entity with an energy buffer (`Capabilities.Energy.BLOCK`, saved as
//! the int `energy`), fills the buffer on every server tick, and tells a player who right-clicks one
//! what it holds and makes. Other mods' machines and cables take the power out of any side.
use horizon_client::mod_api::{
    BlockEntityEnergyEvent, BlockEntityEvent, BlockEntityTickEvent, BlockEntityTicksEvent,
    Compound, Flow, Hooks, Host, ModEnergyBuffer, RightClickBlockEvent, Tag, Text,
};
use std::collections::BTreeMap;
use std::sync::Mutex;

mod generators;
pub use generators::{Generator, Surroundings};

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(GeoPower)
}

/// What one generator holds, in FE.
pub const CAPACITY: i32 = 100_000;
/// The most a machine or cable takes out of a generator in one call.
pub const MAX_EXTRACT: i32 = 1_000;
/// How often (in ticks) a generator looks at its surroundings again; it makes the same power in between.
const SURVEY_TICKS: u32 = 20;

/// What the server knows of a generator between surveys, and what a right-click tells.
#[derive(Clone, Copy, Debug, Default)]
struct Status {
    /// FE per tick from the last survey.
    rate: i32,
    /// FE held after the last tick.
    energy: i32,
    /// Ticks until the next survey.
    survey_in: u32,
}

/// Keyed by dimension and position. Only the server's game ticks block entities, so this holds
/// only the server's generators; a broken one's entry goes on its next right-click or survey miss.
static STATUS: Mutex<BTreeMap<(&'static str, [i32; 3]), Status>> = Mutex::new(BTreeMap::new());

struct GeoPower;

/// The live world through the hooks' `Host`.
struct Live<'a>(&'a dyn Host);

impl Surroundings for Live<'_> {
    fn block(&self, pos: [i32; 3]) -> String {
        self.0.block(pos)
    }
    fn property(&self, pos: [i32; 3], key: &str) -> Option<String> {
        self.0.block_property(pos, key)
    }
    fn sky_light(&self, pos: [i32; 3]) -> u8 {
        self.0.light(pos).1
    }
    fn weather(&self) -> (f32, f32) {
        self.0.weather()
    }
    fn day_time(&self) -> i64 {
        self.0.day_time()
    }
    fn dimension(&self) -> &str {
        self.0.dimension()
    }
}

/// The int `name` of a saved compound (whatever type its keys are), or 0.
fn int_of<K: Ord + for<'s> From<&'s str>>(nbt: &BTreeMap<K, Tag>, name: &str) -> i32 {
    match nbt.get(&K::from(name)) {
        Some(Tag::Int(value)) => *value,
        _ => 0,
    }
}

impl Hooks for GeoPower {
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

    fn block_entity_tick(&self, host: &mut dyn Host, event: &mut BlockEntityTickEvent<'_>) {
        let Some(generator) = Generator::from_id(&event.entity.id) else {
            return;
        };
        let pos = event.pos;
        let world = Live(&*host);
        let key = (host.dimension(), pos);
        let mut status_map = STATUS.lock().unwrap_or_else(|e| e.into_inner());
        let status = status_map.entry(key).or_default();
        if status.survey_in == 0 {
            status.rate = generator.rate(&world, pos);
            status.survey_in = SURVEY_TICKS;
        }
        status.survey_in -= 1;
        let energy = int_of(&event.entity.nbt, "energy").clamp(0, CAPACITY);
        let after = energy.saturating_add(status.rate).min(CAPACITY);
        status.energy = after;
        drop(status_map);
        if after != energy {
            // Only a change goes through `DerefMut`, which marks the entity changed.
            event.entity.nbt.insert("energy".into(), Tag::Int(after));
        }
    }

    fn right_click_block(&self, host: &mut dyn Host, event: &mut RightClickBlockEvent) -> Flow {
        let block = host.block(event.pos);
        let key = (host.dimension(), event.pos);
        let Some(generator) = Generator::from_id(&block) else {
            STATUS.lock().unwrap_or_else(|e| e.into_inner()).remove(&key);
            return Flow::Continue;
        };
        let Some(player) = host.acting_player() else {
            return Flow::Continue;
        };
        let status = STATUS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .copied()
            .unwrap_or_default();
        let name = Text::translate(&format!("block.{block}").replace(':', "."), &[]);
        let numbers = [status.energy, CAPACITY].map(|n| Text::literal(n.to_string()));
        let line = if status.rate > 0 {
            Text::translate(
                "geopower.status",
                &[
                    name,
                    numbers[0].clone(),
                    numbers[1].clone(),
                    Text::literal(status.rate.to_string()),
                ],
            )
        } else {
            let hint = Text::translate(&format!("geopower.hint.{}", generator.path()), &[]);
            Text::translate(
                "geopower.status.idle",
                &[name, numbers[0].clone(), numbers[1].clone(), hint],
            )
        };
        host.action_bar(player.uuid, line);
        // The click goes on: a block in hand is still placed against the generator.
        Flow::Continue
    }
}
