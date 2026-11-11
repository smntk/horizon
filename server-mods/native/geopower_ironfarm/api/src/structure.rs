//! The iron farm multiblock: 3 × 3 wide, 4 high, built as a classic iron farm is, from the bottom up:
//!
//! ```text
//! Layer 1, collection    Layer 2, lava blade   Layer 3, village pod   Layer 4, roof
//!   S C S                  G G G                 H G H                  B B B      front row
//!   S T S                  G L G                 G K G                  B Z B
//!   S P S                  G G G                 H G H                  B B B      back row
//! ```
//!
//! S smooth stone, C the controller, T hopper, P the power port, G glass, L a lava source, H oak log,
//! K composter (the villagers' workstation), B oak planks, Z a zombie head (the zombie that scares the
//! villagers into calling golems). The controller is the middle of the front row; the farm may face
//! any of the four directions. Every layer is the same mirrored, so left and right never matter.
use geopower::world::Surroundings;

pub const CONTROLLER: &str = "geopower:iron_farm_controller";
pub const PORT: &str = "geopower:power_port";

/// The layers from the bottom, each row from the front (the controller's row) to the back.
pub const LAYERS: [[[char; 3]; 3]; 4] = [
    [['S', 'C', 'S'], ['S', 'T', 'S'], ['S', 'P', 'S']],
    [['G', 'G', 'G'], ['G', 'L', 'G'], ['G', 'G', 'G']],
    [['H', 'G', 'H'], ['G', 'K', 'G'], ['H', 'G', 'H']],
    [['B', 'B', 'B'], ['B', 'Z', 'B'], ['B', 'B', 'B']],
];

/// The blocks a letter stands for; the first names it in messages.
pub fn blocks(letter: char) -> &'static [&'static str] {
    match letter {
        'S' => &["minecraft:smooth_stone"],
        'C' => &[CONTROLLER],
        'T' => &["minecraft:hopper"],
        'P' => &[PORT],
        'G' => &["minecraft:glass"],
        'L' => &["minecraft:lava"],
        'H' => &["minecraft:oak_log"],
        'K' => &["minecraft:composter"],
        'B' => &["minecraft:oak_planks"],
        'Z' => &["minecraft:zombie_head", "minecraft:zombie_wall_head"],
        _ => &[],
    }
}

/// The first block that is wrong: its layer, row and column (each from 1), and what belongs there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Issue {
    pub layer: u8,
    pub row: u8,
    pub column: u8,
    pub expected: &'static str,
}

/// A complete farm: where its power port is.
pub type Farm = [i32; 3];

/// The block of a layer, row and column for a controller at `controller` whose farm lies toward `back`.
fn position(controller: [i32; 3], back: [i32; 3], layer: usize, row: usize, column: usize) -> [i32; 3] {
    // Left and right as seen from the front; the layers are mirror-symmetric, so its sign is free.
    let right = [-back[2], 0, back[0]];
    let (row, column, layer) = (row as i32, column as i32 - 1, layer as i32);
    [
        controller[0] + back[0] * row + right[0] * column,
        controller[1] + layer,
        controller[2] + back[2] * row + right[2] * column,
    ]
}

fn fits(world: &dyn Surroundings, pos: [i32; 3], letter: char) -> bool {
    let block = world.block(pos);
    blocks(letter).contains(&block.as_str())
        && (letter != 'L' || world.property(pos, "level").is_none_or(|l| l == "0"))
}

/// Checks the farm around the controller at `controller`, in whichever direction fits best.
pub fn check(world: &dyn Surroundings, controller: [i32; 3]) -> Result<Farm, Issue> {
    let mut best: Option<(usize, Issue)> = None;
    for back in [[0, 0, 1], [0, 0, -1], [1, 0, 0], [-1, 0, 0]] {
        let mut wrong = 0;
        let mut first = None;
        for (layer, rows) in LAYERS.iter().enumerate() {
            for (row, letters) in rows.iter().enumerate() {
                for (column, letter) in letters.iter().enumerate() {
                    if *letter == 'C' || fits(world, position(controller, back, layer, row, column), *letter) {
                        continue;
                    }
                    wrong += 1;
                    if first.is_none() {
                        first = Some(Issue {
                        layer: layer as u8 + 1,
                        row: row as u8 + 1,
                        column: column as u8 + 1,
                        expected: blocks(*letter)[0],
                        });
                    }
                }
            }
        }
        match first {
            None => return Ok(position(controller, back, 0, 2, 1)),
            Some(issue) if best.is_none_or(|(w, _)| wrong < w) => best = Some((wrong, issue)),
            Some(_) => {}
        }
    }
    Err(best.expect("four directions checked").1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct World(HashMap<[i32; 3], String>);

    impl Surroundings for World {
        fn block(&self, pos: [i32; 3]) -> String {
            self.0.get(&pos).cloned().unwrap_or_else(|| "minecraft:air".into())
        }
        fn property(&self, pos: [i32; 3], key: &str) -> Option<String> {
            (key == "level" && self.0.get(&pos).is_some_and(|b| b == "minecraft:lava"))
                .then(|| "0".to_owned())
        }
        fn sky_light(&self, _: [i32; 3]) -> u8 {
            15
        }
        fn weather(&self) -> (f32, f32) {
            (0.0, 0.0)
        }
        fn day_time(&self) -> i64 {
            0
        }
        fn dimension(&self) -> &str {
            "minecraft:overworld"
        }
    }

    /// The farm written out by hand, the controller at (10, 64, 10), its back toward +z (south).
    fn farm_facing_north() -> World {
        let mut world = World::default();
        let rows = |z: i32| z - 10;
        for y in 64..68 {
            for z in 10..13 {
                for x in 9..12 {
                    let corner = x != 10 && z != 11;
                    let edge = !corner && (x != 10 || z != 11);
                    let block = match (y, rows(z), x == 10, corner, edge) {
                        (64, 0, true, _, _) => CONTROLLER,
                        (64, 1, true, _, _) => "minecraft:hopper",
                        (64, 2, true, _, _) => PORT,
                        (64, _, _, _, _) => "minecraft:smooth_stone",
                        (65, 1, true, _, _) => "minecraft:lava",
                        (65, _, _, _, _) => "minecraft:glass",
                        (66, _, _, true, _) => "minecraft:oak_log",
                        (66, _, _, _, true) => "minecraft:glass",
                        (66, _, _, _, _) => "minecraft:composter",
                        (67, 1, true, _, _) => "minecraft:zombie_head",
                        _ => "minecraft:oak_planks",
                    };
                    world.0.insert([x, y, z], block.to_owned());
                }
            }
        }
        world
    }

    #[test]
    fn a_complete_farm_finds_its_power_port() {
        let world = farm_facing_north();
        assert_eq!(check(&world, [10, 64, 10]), Ok([10, 64, 12]));
    }

    #[test]
    fn a_farm_facing_east_fits_too() {
        let north = farm_facing_north();
        // Turn it a quarter: (x, z) around the controller becomes (controller - (z - 10), x).
        let mut east = World::default();
        for (pos, block) in &north.0 {
            let (dx, dz) = (pos[0] - 10, pos[2] - 10);
            east.0.insert([10 - dz, pos[1], 10 + dx], block.clone());
        }
        assert_eq!(check(&east, [10, 64, 10]), Ok([8, 64, 10]));
    }

    #[test]
    fn a_missing_block_is_named_by_layer_row_and_column() {
        let mut world = farm_facing_north();
        // The glass at the back right of layer 2.
        world.0.remove(&[11, 65, 12]);
        let issue = check(&world, [10, 64, 10]).unwrap_err();
        assert_eq!((issue.layer, issue.row, issue.expected), (2, 3, "minecraft:glass"));
        assert!(issue.column == 1 || issue.column == 3);
    }

    #[test]
    fn water_is_not_the_lava_blade() {
        let mut world = farm_facing_north();
        world.0.insert([10, 65, 11], "minecraft:water".to_owned());
        let issue = check(&world, [10, 64, 10]).unwrap_err();
        assert_eq!((issue.layer, issue.row, issue.column), (2, 2, 2));
        assert_eq!(issue.expected, "minecraft:lava");
    }
}
