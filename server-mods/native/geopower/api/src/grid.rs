//! The power grid between GeoPower's blocks.
//!
//! Each block keeps its energy in its own block entity (the int `energy`), and a block entity's tick
//! may change only its own. So energy moves through a ledger: a generator that sends power to a
//! furnace takes it off its own buffer at once and books it for the furnace, which adds what was
//! booked for it at the start of its next tick ([`Grid::settle`]). A block that takes power from another
//! (the iron farm's controller from its power port) books a negative amount the same way. What each
//! block held after its last tick is published ([`Grid::publish`]), so a sender never books more than
//! the receiver has room for, and nothing is lost or made twice.
//!
//! Generators find their receivers by walking the cables ([`crate::cables::survey`]); the route is
//! kept for [`ROUTE_USES`] ticks, so a changed cable takes effect within a second.
use crate::cables::{self, Route};
use crate::world::Surroundings;
use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

/// A block: its dimension and position.
pub type Key = (&'static str, [i32; 3]);

/// How many ticks a generator keeps the route it found before it walks the cables again.
pub const ROUTE_USES: u32 = 20;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Node {
    /// FE after its last tick.
    pub energy: i32,
    pub capacity: i32,
    /// Whether generators may send it power (a furnace, a power port); a generator only gives.
    pub accepts: bool,
}

#[derive(Default)]
pub struct Grid {
    nodes: BTreeMap<Key, Node>,
    /// FE booked for a block and not yet in its buffer: positive sent to it, negative taken from it.
    pending: BTreeMap<Key, i64>,
    routes: BTreeMap<Key, (Route, u32)>,
}

static GRID: Mutex<Grid> = Mutex::new(Grid {
    nodes: BTreeMap::new(),
    pending: BTreeMap::new(),
    routes: BTreeMap::new(),
});

pub fn lock() -> MutexGuard<'static, Grid> {
    GRID.lock().unwrap_or_else(|e| e.into_inner())
}

impl Grid {
    /// The block's energy with what was booked for it since its last tick, kept within its capacity.
    pub fn settle(&mut self, key: Key, energy: i32, capacity: i32) -> i32 {
        let booked = self.pending.remove(&key).unwrap_or(0);
        (energy as i64 + booked).clamp(0, capacity as i64) as i32
    }

    /// What the block holds after its tick, for senders and takers to see.
    pub fn publish(&mut self, key: Key, energy: i32, capacity: i32, accepts: bool) {
        self.nodes.insert(
            key,
            Node {
                energy,
                capacity,
                accepts,
            },
        );
    }

    /// The block's last published state, if it ticked since the world opened.
    pub fn node(&self, key: Key) -> Option<Node> {
        self.nodes.get(&key).copied()
    }

    /// Forgets a block that is gone.
    pub fn remove(&mut self, key: Key) {
        self.nodes.remove(&key);
        self.pending.remove(&key);
        self.routes.remove(&key);
    }

    fn booked(&self, key: Key) -> i64 {
        self.pending.get(&key).copied().unwrap_or(0)
    }

    /// How much more the block can be sent: its room less what is already on its way.
    pub fn room(&self, key: Key) -> i64 {
        match self.nodes.get(&key) {
            Some(node) if node.accepts => {
                (node.capacity as i64 - node.energy as i64 - self.booked(key)).max(0)
            }
            _ => 0,
        }
    }

    /// How much can be taken from the block: what it holds with what is booked for or from it.
    pub fn available(&self, key: Key) -> i64 {
        self.nodes
            .get(&key)
            .map_or(0, |node| (node.energy as i64 + self.booked(key)).max(0))
    }

    /// Books `amount` (negative: taken) for the block's next tick.
    pub fn book(&mut self, key: Key, amount: i64) {
        if amount != 0 {
            *self.pending.entry(key).or_insert(0) += amount;
        }
    }

    /// Whether generators may send the block power.
    pub fn accepts(&self, key: Key) -> bool {
        self.nodes.get(&key).is_some_and(|node| node.accepts)
    }

    /// The generator's route: the cables walked again when it has none or has used it
    /// [`ROUTE_USES`] times. A receiver is a block that published itself as one.
    pub fn route(&mut self, key: Key, world: &dyn Surroundings) -> Route {
        let due = self.routes.get(&key).is_none_or(|(_, uses)| *uses == 0);
        if due {
            let route = cables::survey(world, key.1, |pos| self.accepts((key.0, pos)));
            self.routes.insert(key, (route, ROUTE_USES));
        }
        let entry = self.routes.get_mut(&key).expect("inserted above");
        entry.1 -= 1;
        entry.0.clone()
    }

    /// Sends up to `budget` FE from `from` along `route`, nearest receiver first; what was sent.
    pub fn send(&mut self, from: Key, route: &Route, budget: i64) -> i64 {
        let mut left = budget.min(route.throughput).max(0);
        for target in &route.targets {
            if left == 0 {
                break;
            }
            let key = (from.0, *target);
            let give = self.room(key).min(left);
            self.book(key, give);
            left -= give;
        }
        budget.min(route.throughput).max(0) - left
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIM: &str = "minecraft:overworld";

    fn route(targets: &[[i32; 3]], throughput: i64) -> Route {
        Route {
            targets: targets.to_vec(),
            throughput,
        }
    }

    #[test]
    fn sent_power_arrives_at_the_receivers_next_tick_and_none_is_lost() {
        let mut grid = Grid::default();
        let (generator, furnace) = ((DIM, [0, 0, 0]), (DIM, [2, 0, 0]));
        grid.publish(furnace, 100, 1_000, true);
        let sent = grid.send(generator, &route(&[[2, 0, 0]], 500), 300);
        assert_eq!(sent, 300);
        assert_eq!(grid.settle(furnace, 100, 1_000), 400);
        // Booked once: a second tick adds nothing.
        assert_eq!(grid.settle(furnace, 400, 1_000), 400);
    }

    #[test]
    fn a_receiver_is_never_sent_more_than_its_room() {
        let mut grid = Grid::default();
        let generator = (DIM, [0, 0, 0]);
        let near = (DIM, [1, 0, 0]);
        let far = (DIM, [5, 0, 0]);
        grid.publish(near, 950, 1_000, true);
        grid.publish(far, 0, 1_000, true);
        let path = route(&[[1, 0, 0], [5, 0, 0]], 10_000);
        assert_eq!(grid.send(generator, &path, 200), 200);
        // The near one is full after 50; the rest goes on.
        assert_eq!(grid.room(near), 0);
        assert_eq!(grid.room(far), 850);
        // A second send before either ticked counts what is already booked.
        assert_eq!(grid.send(generator, &path, 2_000), 850);
        assert_eq!(grid.room(far), 0);
    }

    #[test]
    fn the_weakest_cable_limits_what_one_send_moves() {
        let mut grid = Grid::default();
        grid.publish((DIM, [3, 0, 0]), 0, 100_000, true);
        assert_eq!(
            grid.send((DIM, [0, 0, 0]), &route(&[[3, 0, 0]], 200), 5_000),
            200
        );
    }

    #[test]
    fn generators_and_unknown_blocks_take_nothing() {
        let mut grid = Grid::default();
        grid.publish((DIM, [1, 0, 0]), 0, 1_000, false);
        let path = route(&[[1, 0, 0], [9, 9, 9]], 1_000);
        assert_eq!(grid.send((DIM, [0, 0, 0]), &path, 500), 0);
    }

    #[test]
    fn taking_from_a_block_is_booked_against_it() {
        let mut grid = Grid::default();
        let port = (DIM, [0, 0, 0]);
        grid.publish(port, 100, 1_000, true);
        grid.book(port, -40);
        assert_eq!(grid.available(port), 60);
        assert_eq!(grid.settle(port, 100, 1_000), 60);
    }
}
