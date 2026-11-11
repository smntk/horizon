//! Electric furnaces: a furnace, a blast furnace and a smoker that run on power instead of fuel. They
//! use the vanilla furnace screen: the flame shows how full the buffer is, the arrow the progress.
//! Slot 0 takes the input, slot 2 holds the output; the fuel slot (1) is not used.
use crate::grid;
use crate::smelting::{self, Kind};
use crate::world::{int_of, store};
use horizon_client::mod_api::{Host, ItemStack, ModBlockEntity, ModContainer, ModEnergyBuffer};

/// FE one furnace holds.
pub const CAPACITY: i32 = 20_000;
/// The most FE another mod's cable puts in at once (GeoPower's own grid books it directly).
pub const MAX_INSERT: i32 = 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Furnace {
    Electric,
    Blast,
    Smoker,
}

impl Furnace {
    pub const ALL: [Furnace; 3] = [Furnace::Electric, Furnace::Blast, Furnace::Smoker];

    pub fn id(self) -> &'static str {
        match self {
            Furnace::Electric => "geopower:electric_furnace",
            Furnace::Blast => "geopower:electric_blast_furnace",
            Furnace::Smoker => "geopower:electric_smoker",
        }
    }

    pub fn from_id(id: &str) -> Option<Furnace> {
        Furnace::ALL.into_iter().find(|f| f.id() == id)
    }

    /// FE per tick while it works.
    pub fn usage(self) -> i32 {
        match self {
            Furnace::Electric => 20,
            Furnace::Blast => 40,
            Furnace::Smoker => 20,
        }
    }

    /// Ticks per item: twice as fast as a vanilla furnace, four times for the special ones.
    pub fn ticks(self) -> i32 {
        match self {
            Furnace::Electric => 100,
            Furnace::Blast | Furnace::Smoker => 50,
        }
    }

    fn kinds(self) -> &'static [Kind] {
        match self {
            Furnace::Electric => &[Kind::Ore, Kind::Food, Kind::Other],
            Furnace::Blast => &[Kind::Ore],
            Furnace::Smoker => &[Kind::Food],
        }
    }

    /// The vanilla menu it opens (its template opens it on use).
    fn menu(self) -> &'static str {
        match self {
            Furnace::Electric => "minecraft:furnace",
            Furnace::Blast => "minecraft:blast_furnace",
            Furnace::Smoker => "minecraft:smoker",
        }
    }

    pub fn container(self) -> ModContainer {
        ModContainer {
            slots: 3,
            // Down, up, north, south, west, east: hoppers above and beside fill the input, one below
            // takes the output.
            faces: [vec![2], vec![0], vec![0], vec![0], vec![0], vec![0]],
            inputs: vec![0],
            outputs: vec![2],
            menu: Some(self.menu().to_owned()),
        }
    }

    pub fn buffer() -> ModEnergyBuffer {
        ModEnergyBuffer {
            capacity: CAPACITY,
            max_insert: MAX_INSERT,
            max_extract: 0,
            insert_faces: [true; 6],
            extract_faces: [false; 6],
        }
    }
}

/// What one tick of a furnace does to its slots, energy and progress.
#[derive(Debug, PartialEq)]
pub struct Step {
    pub energy: i32,
    pub progress: i32,
    pub running: bool,
    /// The input's and output's new stacks when an item was finished.
    pub finished: Option<(Option<ItemStack>, ItemStack)>,
}

/// One tick: with a recipe for the input, room in the output and power for the tick, it works on;
/// an item is done after [`Furnace::ticks`]. Without a recipe or room the progress starts over.
pub fn step(
    furnace: Furnace,
    energy: i32,
    progress: i32,
    input: Option<&ItemStack>,
    output: Option<&ItemStack>,
) -> Step {
    let result = input.and_then(|s| smelting::result(&s.item, furnace.kinds()));
    let Some(result) = result.map(|r| format!("minecraft:{r}")) else {
        return Step {
            energy,
            progress: 0,
            running: false,
            finished: None,
        };
    };
    let room = output.is_none_or(|o| *o.item == *result && o.count < 64);
    if !room || energy < furnace.usage() {
        return Step {
            energy,
            progress: if room { progress } else { 0 },
            running: false,
            finished: None,
        };
    }
    let energy = energy - furnace.usage();
    let progress = progress + 1;
    if progress < furnace.ticks() {
        return Step {
            energy,
            progress,
            running: true,
            finished: None,
        };
    }
    let input = input.expect("a recipe needs an input");
    let left = (input.count > 1).then(|| ItemStack {
        count: input.count - 1,
        ..input.clone()
    });
    let made = match output {
        Some(o) => ItemStack {
            count: o.count + 1,
            ..o.clone()
        },
        None => ItemStack {
            item: result.as_str().into(),
            count: 1,
            damage: 0,
            extra: None,
        },
    };
    Step {
        energy,
        progress: 0,
        running: true,
        finished: Some((left, made)),
    }
}

/// A furnace's server tick.
pub fn tick(furnace: Furnace, host: &mut dyn Host, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if entity.items.len() < 3 {
        return;
    }
    let key = (host.dimension(), pos);
    let mut grid = grid::lock();
    let energy = grid.settle(key, int_of(&entity.nbt, "energy"), CAPACITY);
    let step = step(
        furnace,
        energy,
        int_of(&entity.nbt, "progress"),
        entity.items[0].as_ref(),
        entity.items[2].as_ref(),
    );
    grid.publish(key, step.energy, CAPACITY, true);
    drop(grid);

    store(entity, "energy", step.energy);
    store(entity, "progress", step.progress);
    if let Some((input, output)) = step.finished {
        entity.items[0] = input;
        entity.items[2] = Some(output);
    }
    // The furnace screen: flame = data 0 of data 1 (the charge), arrow = data 2 of data 3.
    let data = vec![
        (step.energy as i64 * 100 / CAPACITY as i64) as i32,
        100,
        step.progress,
        furnace.ticks(),
    ];
    if entity.data != data {
        entity.data = data;
    }
    let lit = if step.running { "true" } else { "false" };
    if host.block_property(pos, "lit").as_deref() != Some(lit) {
        host.set_block_property(pos, "lit", lit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(item: &str, count: u32) -> ItemStack {
        ItemStack {
            item: item.into(),
            count,
            damage: 0,
            extra: None,
        }
    }

    #[test]
    fn an_item_takes_its_ticks_and_their_power() {
        let raw = stack("minecraft:raw_iron", 2);
        let mut energy = 10_000;
        let mut progress = 0;
        for _ in 0..99 {
            let s = step(Furnace::Electric, energy, progress, Some(&raw), None);
            assert!(s.running && s.finished.is_none());
            (energy, progress) = (s.energy, s.progress);
        }
        let done = step(Furnace::Electric, energy, progress, Some(&raw), None);
        // 100 ticks of 20 FE.
        assert_eq!(done.energy, 10_000 - 2_000);
        assert_eq!(done.progress, 0);
        let (left, made) = done.finished.unwrap();
        assert_eq!(left, Some(stack("minecraft:raw_iron", 1)));
        assert_eq!(made, stack("minecraft:iron_ingot", 1));
    }

    #[test]
    fn the_last_input_leaves_the_slot_empty_and_the_output_stacks() {
        let s = step(
            Furnace::Blast,
            1_000,
            49,
            Some(&stack("minecraft:raw_gold", 1)),
            Some(&stack("minecraft:gold_ingot", 5)),
        );
        assert_eq!(
            s.finished,
            Some((None, stack("minecraft:gold_ingot", 6)))
        );
    }

    #[test]
    fn it_waits_without_power_and_starts_over_without_room() {
        let raw = stack("minecraft:raw_iron", 1);
        let waiting = step(Furnace::Electric, 19, 40, Some(&raw), None);
        assert_eq!((waiting.running, waiting.progress, waiting.energy), (false, 40, 19));
        let full = step(
            Furnace::Electric,
            5_000,
            40,
            Some(&raw),
            Some(&stack("minecraft:iron_ingot", 64)),
        );
        assert_eq!((full.running, full.progress), (false, 0));
        let other = step(
            Furnace::Electric,
            5_000,
            40,
            Some(&raw),
            Some(&stack("minecraft:gold_ingot", 1)),
        );
        assert!(!other.running);
    }

    #[test]
    fn a_smoker_cooks_food_only() {
        let s = step(Furnace::Smoker, 5_000, 0, Some(&stack("minecraft:raw_iron", 1)), None);
        assert!(!s.running);
        let s = step(Furnace::Smoker, 5_000, 0, Some(&stack("minecraft:beef", 1)), None);
        assert!(s.running);
    }
}
