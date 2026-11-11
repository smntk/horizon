//! What the electric furnaces make of what. Vanilla's smelting, blasting and smoking recipes, for
//! the inputs players use (the hook API reads only mods' own recipe types on a server, not
//! vanilla's): ores and raw metal (furnace and blast furnace), food (furnace and smoker), and the
//! rest of the common furnace recipes (furnace only).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Smelting and blasting.
    Ore,
    /// Smelting and smoking.
    Food,
    /// Smelting only.
    Other,
}

use Kind::{Food, Ore, Other};

/// Input, result (both without `minecraft:`), kind.
const RECIPES: &[(&str, &str, Kind)] = &[
    ("raw_iron", "iron_ingot", Ore),
    ("raw_gold", "gold_ingot", Ore),
    ("raw_copper", "copper_ingot", Ore),
    ("iron_ore", "iron_ingot", Ore),
    ("deepslate_iron_ore", "iron_ingot", Ore),
    ("gold_ore", "gold_ingot", Ore),
    ("deepslate_gold_ore", "gold_ingot", Ore),
    ("nether_gold_ore", "gold_ingot", Ore),
    ("copper_ore", "copper_ingot", Ore),
    ("deepslate_copper_ore", "copper_ingot", Ore),
    ("coal_ore", "coal", Ore),
    ("deepslate_coal_ore", "coal", Ore),
    ("diamond_ore", "diamond", Ore),
    ("deepslate_diamond_ore", "diamond", Ore),
    ("emerald_ore", "emerald", Ore),
    ("deepslate_emerald_ore", "emerald", Ore),
    ("lapis_ore", "lapis_lazuli", Ore),
    ("deepslate_lapis_ore", "lapis_lazuli", Ore),
    ("redstone_ore", "redstone", Ore),
    ("deepslate_redstone_ore", "redstone", Ore),
    ("nether_quartz_ore", "quartz", Ore),
    ("ancient_debris", "netherite_scrap", Ore),
    ("beef", "cooked_beef", Food),
    ("porkchop", "cooked_porkchop", Food),
    ("chicken", "cooked_chicken", Food),
    ("mutton", "cooked_mutton", Food),
    ("rabbit", "cooked_rabbit", Food),
    ("cod", "cooked_cod", Food),
    ("salmon", "cooked_salmon", Food),
    ("potato", "baked_potato", Food),
    ("kelp", "dried_kelp", Food),
    ("sand", "glass", Other),
    ("red_sand", "glass", Other),
    ("cobblestone", "stone", Other),
    ("stone", "smooth_stone", Other),
    ("cobbled_deepslate", "deepslate", Other),
    ("stone_bricks", "cracked_stone_bricks", Other),
    ("deepslate_bricks", "cracked_deepslate_bricks", Other),
    ("deepslate_tiles", "cracked_deepslate_tiles", Other),
    ("sandstone", "smooth_sandstone", Other),
    ("red_sandstone", "smooth_red_sandstone", Other),
    ("quartz_block", "smooth_quartz", Other),
    ("basalt", "smooth_basalt", Other),
    ("clay_ball", "brick", Other),
    ("clay", "terracotta", Other),
    ("netherrack", "nether_brick", Other),
    ("nether_bricks", "cracked_nether_bricks", Other),
    ("wet_sponge", "sponge", Other),
    ("cactus", "green_dye", Other),
    ("sea_pickle", "lime_dye", Other),
    ("chorus_fruit", "popped_chorus_fruit", Other),
    ("resin_clump", "resin_brick", Other),
    ("oak_log", "charcoal", Other),
    ("spruce_log", "charcoal", Other),
    ("birch_log", "charcoal", Other),
    ("jungle_log", "charcoal", Other),
    ("acacia_log", "charcoal", Other),
    ("dark_oak_log", "charcoal", Other),
    ("mangrove_log", "charcoal", Other),
    ("cherry_log", "charcoal", Other),
    ("pale_oak_log", "charcoal", Other),
];

/// What `item` (`minecraft:raw_iron`) becomes in a furnace that takes `kinds`.
pub fn result(item: &str, kinds: &[Kind]) -> Option<&'static str> {
    let name = item.strip_prefix("minecraft:")?;
    RECIPES
        .iter()
        .find(|(input, _, kind)| *input == name && kinds.contains(kind))
        .map(|(_, output, _)| *output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_furnace_takes_only_its_kinds() {
        let all = [Ore, Food, Other];
        assert_eq!(result("minecraft:raw_iron", &all), Some("iron_ingot"));
        assert_eq!(result("minecraft:raw_iron", &[Ore]), Some("iron_ingot"));
        assert_eq!(result("minecraft:beef", &[Ore]), None);
        assert_eq!(result("minecraft:beef", &[Food]), Some("cooked_beef"));
        assert_eq!(result("minecraft:sand", &[Ore, Food]), None);
        assert_eq!(result("minecraft:diamond", &all), None);
        assert_eq!(result("othermod:raw_tin", &all), None);
    }
}
