//! How much power each generator makes where it stands: plain functions of what is around it, so
//! they are tested without a game.
use geopower::world::{SIDES, Surroundings, offset};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Generator {
    Solar,
    Wind,
    Hydro,
    Geothermal,
}

/// FE per tick of a solar panel at noon under a clear, open sky.
pub const SOLAR_PEAK: f64 = 20.0;
/// The most a wind turbine's height gives (reached at y 127).
pub const WIND_MAX: f64 = 16.0;
/// Sea level: a wind turbine at or below it stands still.
pub const SEA_LEVEL: i32 = 63;

/// A fluid block's `level`: 0 a source, 1 to 7 flowing, 8 and up falling.
fn level(world: &dyn Surroundings, pos: [i32; 3]) -> u8 {
    world
        .property(pos, "level")
        .and_then(|l| l.parse().ok())
        .unwrap_or(0)
}

impl Generator {
    pub const ALL: [Generator; 4] = [
        Generator::Solar,
        Generator::Wind,
        Generator::Hydro,
        Generator::Geothermal,
    ];

    /// The block's path in the `geopower` namespace.
    pub fn path(self) -> &'static str {
        match self {
            Generator::Solar => "solar_panel",
            Generator::Wind => "wind_turbine",
            Generator::Hydro => "hydro_generator",
            Generator::Geothermal => "geothermal_generator",
        }
    }

    /// The generator a block (or its block entity) id names, if any.
    pub fn from_id(id: &str) -> Option<Generator> {
        let path = id.strip_prefix("geopower:")?;
        Generator::ALL.into_iter().find(|g| g.path() == path)
    }

    /// FE per tick at `pos` now.
    pub fn rate(self, world: &dyn Surroundings, pos: [i32; 3]) -> i32 {
        let rate = match self {
            Generator::Solar => solar(world, pos),
            Generator::Wind => wind(world, pos),
            Generator::Hydro => hydro(world, pos),
            Generator::Geothermal => geothermal(world, pos),
        };
        rate.round().clamp(0.0, i32::MAX as f64) as i32
    }
}

/// Sunlight: none at night (day time 12000 to 24000) or without sky above; a sine over the day
/// peaking at noon (6000), scaled by the sky light above and dimmed by rain and thunder.
fn solar(world: &dyn Surroundings, pos: [i32; 3]) -> f64 {
    let sky = world.sky_light(offset(pos, [0, 1, 0]));
    let time = world.day_time().rem_euclid(24_000);
    if sky == 0 || time >= 12_000 {
        return 0.0;
    }
    let sun = (PI * time as f64 / 12_000.0).sin();
    let (rain, thunder) = world.weather();
    let weather = 1.0 - 0.5 * rain as f64 - 0.25 * thunder as f64;
    SOLAR_PEAK * sun * (sky as f64 / 15.0) * weather
}

/// Wind: a quarter FE per block above sea level (up to [`WIND_MAX`]), times the share of open air
/// in the eight blocks around it and the one above, times a slow gust, times the weather (a storm
/// doubles it).
fn wind(world: &dyn Surroundings, pos: [i32; 3]) -> f64 {
    let height = pos[1] - SEA_LEVEL;
    if height <= 0 {
        return 0.0;
    }
    let mut around = vec![offset(pos, [0, 1, 0])];
    for dx in -1..=1 {
        for dz in -1..=1 {
            if (dx, dz) != (0, 0) {
                around.push(offset(pos, [dx, 0, dz]));
            }
        }
    }
    let open = around
        .iter()
        .filter(|p| world.block(**p) == "minecraft:air")
        .count();
    if open == 0 {
        return 0.0;
    }
    let base = (height as f64 / 4.0).min(WIND_MAX);
    let (rain, thunder) = world.weather();
    let weather = 1.0 + 0.5 * rain as f64 + 0.5 * thunder as f64;
    base * (open as f64 / around.len() as f64) * gust(world.day_time(), pos) * weather
}

/// 0.6 to 1.0: the wind rises and falls over about ten minutes, not at once for every turbine.
fn gust(day_time: i64, pos: [i32; 3]) -> f64 {
    let phase = (pos[0].wrapping_mul(31).wrapping_add(pos[2].wrapping_mul(17))).rem_euclid(628);
    0.8 + 0.2 * (day_time as f64 / 1_000.0 + phase as f64 / 100.0).sin()
}

/// Water on any side: a still source gives 1, flowing water 4, falling water and bubble columns 6.
fn hydro(world: &dyn Surroundings, pos: [i32; 3]) -> f64 {
    SIDES
        .iter()
        .map(|side| {
            let at = offset(pos, *side);
            match world.block(at).as_str() {
                "minecraft:water" => match level(world, at) {
                    0 => 1.0,
                    1..=7 => 4.0,
                    _ => 6.0,
                },
                "minecraft:bubble_column" => 6.0,
                _ => 0.0,
            }
        })
        .sum()
}

/// Heat on any side: a lava source 8, flowing lava 4, a lava cauldron 6, magma 2, fire 1. Below y 0
/// it is half again as much, and in the Nether a quarter more.
fn geothermal(world: &dyn Surroundings, pos: [i32; 3]) -> f64 {
    let heat: f64 = SIDES
        .iter()
        .map(|side| {
            let at = offset(pos, *side);
            match world.block(at).as_str() {
                "minecraft:lava" if level(world, at) == 0 => 8.0,
                "minecraft:lava" => 4.0,
                "minecraft:lava_cauldron" => 6.0,
                "minecraft:magma_block" => 2.0,
                "minecraft:fire" | "minecraft:soul_fire" => 1.0,
                _ => 0.0,
            }
        })
        .sum();
    let depth = if pos[1] < 0 { 1.5 } else { 1.0 };
    let nether = if world.dimension() == "minecraft:the_nether" {
        1.25
    } else {
        1.0
    };
    heat * depth * nether
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A world of air under open sky, with the blocks a test puts in it.
    struct World {
        blocks: HashMap<[i32; 3], (&'static str, Option<&'static str>)>,
        sky: u8,
        weather: (f32, f32),
        day_time: i64,
        dimension: &'static str,
    }

    impl World {
        fn new() -> Self {
            World {
                blocks: HashMap::new(),
                sky: 15,
                weather: (0.0, 0.0),
                day_time: 6_000,
                dimension: "minecraft:overworld",
            }
        }
        fn put(&mut self, pos: [i32; 3], block: &'static str, level: Option<&'static str>) {
            self.blocks.insert(pos, (block, level));
        }
    }

    impl Surroundings for World {
        fn block(&self, pos: [i32; 3]) -> String {
            self.blocks.get(&pos).map_or("minecraft:air", |b| b.0).to_owned()
        }
        fn property(&self, pos: [i32; 3], key: &str) -> Option<String> {
            (key == "level")
                .then(|| self.blocks.get(&pos).and_then(|b| b.1))
                .flatten()
                .map(str::to_owned)
        }
        fn sky_light(&self, _pos: [i32; 3]) -> u8 {
            self.sky
        }
        fn weather(&self) -> (f32, f32) {
            self.weather
        }
        fn day_time(&self) -> i64 {
            self.day_time
        }
        fn dimension(&self) -> &str {
            self.dimension
        }
    }

    #[test]
    fn ids_name_only_the_four_generators() {
        assert_eq!(
            Generator::from_id("geopower:solar_panel"),
            Some(Generator::Solar)
        );
        assert_eq!(
            Generator::from_id("geopower:geothermal_generator"),
            Some(Generator::Geothermal)
        );
        assert_eq!(Generator::from_id("minecraft:solar_panel"), None);
        assert_eq!(Generator::from_id("geopower:battery"), None);
    }

    #[test]
    fn solar_peaks_at_noon_and_stops_at_night_or_under_cover() {
        let mut world = World::new();
        let pos = [0, 70, 0];
        assert_eq!(Generator::Solar.rate(&world, pos), 20);
        // Day 3, noon: the clock keeps counting.
        world.day_time = 2 * 24_000 + 6_000;
        assert_eq!(Generator::Solar.rate(&world, pos), 20);
        // Morning, a sixth into the day: sin(30°) = 0.5.
        world.day_time = 2_000;
        assert_eq!(Generator::Solar.rate(&world, pos), 10);
        world.day_time = 18_000;
        assert_eq!(Generator::Solar.rate(&world, pos), 0);
        world.day_time = 6_000;
        world.sky = 0;
        assert_eq!(Generator::Solar.rate(&world, pos), 0);
    }

    #[test]
    fn rain_and_thunder_dim_the_sun() {
        let mut world = World::new();
        world.weather = (1.0, 0.0);
        assert_eq!(Generator::Solar.rate(&world, [0, 70, 0]), 10);
        world.weather = (1.0, 1.0);
        assert_eq!(Generator::Solar.rate(&world, [0, 70, 0]), 5);
    }

    #[test]
    fn wind_needs_height_and_open_air() {
        let mut world = World::new();
        // At (0, _, 0) the gust's phase is 0; day time 0 puts the gust at 0.8.
        world.day_time = 0;
        assert_eq!(Generator::Wind.rate(&world, [0, 63, 0]), 0);
        // 64 blocks above sea level: the full 16, times 0.8.
        assert_eq!(Generator::Wind.rate(&world, [0, 127, 0]), 13);
        // Higher gives no more.
        assert_eq!(Generator::Wind.rate(&world, [0, 300, 0]), 13);
        // Walled in on all sides and above: nothing.
        for dx in -1..=1 {
            for dz in -1..=1 {
                world.put([dx, 127, dz], "minecraft:stone", None);
            }
        }
        world.put([0, 128, 0], "minecraft:stone", None);
        world.blocks.remove(&[0, 127, 0]);
        assert_eq!(Generator::Wind.rate(&world, [0, 127, 0]), 0);
    }

    #[test]
    fn storms_double_the_wind() {
        let mut world = World::new();
        world.day_time = 0;
        world.weather = (1.0, 1.0);
        // 16 × 0.8 × 2.
        assert_eq!(Generator::Wind.rate(&world, [0, 127, 0]), 26);
    }

    #[test]
    fn hydro_counts_moving_water_most() {
        let mut world = World::new();
        let pos = [5, 40, 5];
        assert_eq!(Generator::Hydro.rate(&world, pos), 0);
        world.put([4, 40, 5], "minecraft:water", Some("0"));
        assert_eq!(Generator::Hydro.rate(&world, pos), 1);
        world.put([6, 40, 5], "minecraft:water", Some("3"));
        assert_eq!(Generator::Hydro.rate(&world, pos), 5);
        world.put([5, 41, 5], "minecraft:water", Some("8"));
        assert_eq!(Generator::Hydro.rate(&world, pos), 11);
    }

    #[test]
    fn geothermal_is_hotter_deep_down_and_in_the_nether() {
        let mut world = World::new();
        let pos = [0, 10, 0];
        world.put([0, 9, 0], "minecraft:lava", Some("0"));
        world.put([1, 10, 0], "minecraft:magma_block", None);
        assert_eq!(Generator::Geothermal.rate(&world, pos), 10);
        world.put([-1, 10, 0], "minecraft:lava", Some("2"));
        assert_eq!(Generator::Geothermal.rate(&world, pos), 14);

        let mut deep = World::new();
        let pos = [0, -20, 0];
        deep.put([0, -21, 0], "minecraft:lava", Some("0"));
        assert_eq!(Generator::Geothermal.rate(&deep, pos), 12);

        let mut nether = World::new();
        nether.dimension = "minecraft:the_nether";
        nether.put([0, 31, 0], "minecraft:lava", Some("0"));
        assert_eq!(Generator::Geothermal.rate(&nether, [0, 32, 0]), 10);
    }
}
