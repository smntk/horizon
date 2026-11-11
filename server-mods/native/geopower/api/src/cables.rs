//! Cables, after Mekanism's universal cables: four tiers, one block (`geopower:cable`, its state
//! `tier`), upgraded in place by right-clicking one with an alloy, which upgrades every cable of the
//! same tier joined to it. A generator's power goes along the cables to the nearest receivers, at
//! most what the weakest cable on the way carries in a tick.
use crate::world::{SIDES, Surroundings, offset};
use std::collections::{BTreeSet, VecDeque};

pub const CABLE: &str = "geopower:cable";
/// The most cables one generator's walk follows, and one alloy upgrades.
pub const MAX_CABLES: usize = 2_048;
pub const MAX_UPGRADE: usize = 64;
/// What a generator sends to a receiver touching it, with no cable between.
pub const DIRECT: i64 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Basic,
    Advanced,
    Elite,
    Ultimate,
}

impl Tier {
    pub const ALL: [Tier; 4] = [Tier::Basic, Tier::Advanced, Tier::Elite, Tier::Ultimate];

    /// The `tier` state's value.
    pub fn name(self) -> &'static str {
        match self {
            Tier::Basic => "basic",
            Tier::Advanced => "advanced",
            Tier::Elite => "elite",
            Tier::Ultimate => "ultimate",
        }
    }

    pub fn from_name(name: &str) -> Option<Tier> {
        Tier::ALL.into_iter().find(|t| t.name() == name)
    }

    /// FE a cable of this tier carries per tick. Mekanism's tiers go up 16×, 8×, 8× from 8 000 FE/t;
    /// GeoPower's generators make tens of FE/t, so these are scaled to them, 4× a tier.
    pub fn throughput(self) -> i64 {
        match self {
            Tier::Basic => 200,
            Tier::Advanced => 800,
            Tier::Elite => 3_200,
            Tier::Ultimate => 12_800,
        }
    }

    /// The alloy that upgrades a cable of the tier below to this one.
    pub fn alloy(self) -> Option<&'static str> {
        match self {
            Tier::Basic => None,
            Tier::Advanced => Some("geopower:infused_alloy"),
            Tier::Elite => Some("geopower:reinforced_alloy"),
            Tier::Ultimate => Some("geopower:atomic_alloy"),
        }
    }

    /// The tier an alloy upgrades to.
    pub fn upgraded_by(item: &str) -> Option<Tier> {
        Tier::ALL.into_iter().find(|t| t.alloy() == Some(item))
    }

    pub fn below(self) -> Option<Tier> {
        match self {
            Tier::Basic => None,
            Tier::Advanced => Some(Tier::Basic),
            Tier::Elite => Some(Tier::Advanced),
            Tier::Ultimate => Some(Tier::Elite),
        }
    }
}

/// The cable's tier at `pos`, or none where there is no cable.
pub fn tier_at(world: &dyn Surroundings, pos: [i32; 3]) -> Option<Tier> {
    (world.block(pos) == CABLE).then(|| {
        world
            .property(pos, "tier")
            .and_then(|t| Tier::from_name(&t))
            .unwrap_or(Tier::Basic)
    })
}

/// Where a generator's power goes: its receivers, nearest first, and what one tick may move.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Route {
    pub targets: Vec<[i32; 3]>,
    pub throughput: i64,
}

/// Walks the cables from the generator at `from`: every receiver (`receives`) touching it or a cable
/// joined to it, nearest first. The weakest cable sets the throughput.
pub fn survey(
    world: &dyn Surroundings,
    from: [i32; 3],
    receives: impl Fn([i32; 3]) -> bool,
) -> Route {
    let mut seen = BTreeSet::from([from]);
    let mut queue = VecDeque::from([from]);
    let mut targets = Vec::new();
    let mut weakest: Option<i64> = None;
    let mut cables = 0;
    'walk: while let Some(at) = queue.pop_front() {
        for side in SIDES {
            let next = offset(at, side);
            if !seen.insert(next) {
                continue;
            }
            if let Some(tier) = tier_at(world, next) {
                cables += 1;
                if cables > MAX_CABLES {
                    break 'walk;
                }
                weakest = Some(weakest.map_or(tier.throughput(), |w| w.min(tier.throughput())));
                queue.push_back(next);
            } else if receives(next) {
                targets.push(next);
            }
        }
    }
    Route {
        targets,
        throughput: weakest.unwrap_or(DIRECT),
    }
}

/// The cables of tier `tier` joined to the one at `start` (itself first), at most [`MAX_UPGRADE`].
pub fn joined_of_tier(world: &dyn Surroundings, start: [i32; 3], tier: Tier) -> Vec<[i32; 3]> {
    let mut found = Vec::new();
    if tier_at(world, start) != Some(tier) {
        return found;
    }
    let mut seen = BTreeSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(at) = queue.pop_front() {
        found.push(at);
        if found.len() == MAX_UPGRADE {
            break;
        }
        for side in SIDES {
            let next = offset(at, side);
            if seen.insert(next) && tier_at(world, next) == Some(tier) {
                queue.push_back(next);
            }
        }
    }
    found
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Blocks with an optional property value (water's `level`, a cable's `tier`), air elsewhere.
    #[derive(Default)]
    pub struct World(pub HashMap<[i32; 3], (&'static str, Option<&'static str>)>);

    impl World {
        pub fn put(&mut self, pos: [i32; 3], block: &'static str, value: Option<&'static str>) {
            self.0.insert(pos, (block, value));
        }
    }

    impl Surroundings for World {
        fn block(&self, pos: [i32; 3]) -> String {
            self.0.get(&pos).map_or("minecraft:air", |b| b.0).to_owned()
        }
        fn property(&self, pos: [i32; 3], _key: &str) -> Option<String> {
            self.0.get(&pos).and_then(|b| b.1).map(str::to_owned)
        }
        fn sky_light(&self, _pos: [i32; 3]) -> u8 {
            15
        }
        fn weather(&self) -> (f32, f32) {
            (0.0, 0.0)
        }
        fn day_time(&self) -> i64 {
            6_000
        }
        fn dimension(&self) -> &str {
            "minecraft:overworld"
        }
    }

    fn line(world: &mut World, from: i32, to: i32, tier: &'static str) {
        for x in from..=to {
            world.put([x, 0, 0], CABLE, Some(tier));
        }
    }

    #[test]
    fn alloys_upgrade_one_tier_each_as_in_mekanism() {
        assert_eq!(
            Tier::upgraded_by("geopower:infused_alloy"),
            Some(Tier::Advanced)
        );
        assert_eq!(
            Tier::upgraded_by("geopower:atomic_alloy"),
            Some(Tier::Ultimate)
        );
        assert_eq!(Tier::upgraded_by("minecraft:iron_ingot"), None);
        assert_eq!(Tier::Ultimate.below(), Some(Tier::Elite));
    }

    #[test]
    fn power_follows_the_cables_to_the_nearest_receivers_first() {
        let mut world = World::default();
        // Generator at x 0, cables from 1 to 6, furnaces at x 3 (beside the cable) and at x 7.
        line(&mut world, 1, 6, "basic");
        let receivers = [[3, 1, 0], [7, 0, 0]];
        let route = survey(&world, [0, 0, 0], |p| receivers.contains(&p));
        assert_eq!(route.targets, vec![[3, 1, 0], [7, 0, 0]]);
        assert_eq!(route.throughput, 200);
    }

    #[test]
    fn the_weakest_cable_sets_the_throughput() {
        let mut world = World::default();
        line(&mut world, 1, 3, "ultimate");
        world.put([4, 0, 0], CABLE, Some("advanced"));
        line(&mut world, 5, 6, "elite");
        let route = survey(&world, [0, 0, 0], |p| p == [7, 0, 0]);
        assert_eq!(route.targets, vec![[7, 0, 0]]);
        assert_eq!(route.throughput, 800);
    }

    #[test]
    fn a_receiver_touching_the_generator_needs_no_cable() {
        let world = World::default();
        let route = survey(&world, [0, 0, 0], |p| p == [0, 1, 0]);
        assert_eq!(route.targets, vec![[0, 1, 0]]);
        assert_eq!(route.throughput, DIRECT);
    }

    #[test]
    fn power_does_not_pass_through_a_receiver() {
        let mut world = World::default();
        // Generator, furnace at x 1, then cables beyond it leading to another furnace.
        line(&mut world, 2, 3, "basic");
        let receivers = [[1, 0, 0], [4, 0, 0]];
        let route = survey(&world, [0, 0, 0], |p| receivers.contains(&p));
        assert_eq!(route.targets, vec![[1, 0, 0]]);
    }

    #[test]
    fn an_upgrade_takes_the_joined_cables_of_the_same_tier_only() {
        let mut world = World::default();
        line(&mut world, 0, 4, "basic");
        world.put([5, 0, 0], CABLE, Some("advanced"));
        line(&mut world, 6, 8, "basic");
        let cables = joined_of_tier(&world, [2, 0, 0], Tier::Basic);
        assert_eq!(cables.len(), 5);
        assert!(!cables.contains(&[6, 0, 0]));
        assert!(joined_of_tier(&world, [2, 0, 0], Tier::Elite).is_empty());
    }
}
