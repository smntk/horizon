//! What GeoPower's blocks read of the world, behind a trait so their logic is tested without a game.
use horizon_client::mod_api::{Host, ModBlockEntity, Tag, Text};
use std::collections::BTreeMap;

/// What a GeoPower block reads of the world around it.
pub trait Surroundings {
    /// The block's id at `pos` (`minecraft:water`).
    fn block(&self, pos: [i32; 3]) -> String;
    /// A state property of the block at `pos` (water's `level`, a cable's `tier`).
    fn property(&self, pos: [i32; 3], key: &str) -> Option<String>;
    /// The sky light at `pos`, 0 to 15.
    fn sky_light(&self, pos: [i32; 3]) -> u8;
    /// Rain and thunder levels, each 0.0 to 1.0.
    fn weather(&self) -> (f32, f32);
    /// The time of day in ticks; it keeps counting past 24000.
    fn day_time(&self) -> i64;
    /// `minecraft:overworld`, `minecraft:the_nether`...
    fn dimension(&self) -> &str;
}

/// The live world through a hook's `Host`.
pub struct Live<'a>(pub &'a dyn Host);

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

/// The six sides: down, up, north, south, west, east.
pub const SIDES: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];

pub fn offset(pos: [i32; 3], by: [i32; 3]) -> [i32; 3] {
    [pos[0] + by[0], pos[1] + by[1], pos[2] + by[2]]
}

/// The int `name` of a saved compound (whatever type its keys are), or 0.
pub fn int_of<K: Ord + for<'s> From<&'s str>>(nbt: &BTreeMap<K, Tag>, name: &str) -> i32 {
    match nbt.get(&K::from(name)) {
        Some(Tag::Int(value)) => *value,
        _ => 0,
    }
}

/// Sets the int `name` of a block entity's saved compound when it changes (a missing one reads 0).
/// Only a write goes through `DerefMut`, which marks the entity changed.
pub fn store(entity: &mut ModBlockEntity, name: &str, value: i32) {
    if int_of(&entity.nbt, name) != value {
        entity.nbt.insert(name.into(), Tag::Int(value));
    }
}

/// A block's translated name (`block.minecraft.glass`), resolved by each player's language.
pub fn block_name(id: &str) -> Text {
    Text::translate(&format!("block.{}", id.replace(':', ".")), &[])
}

/// A number as text.
pub fn num(n: impl std::fmt::Display) -> Text {
    Text::literal(n.to_string())
}
