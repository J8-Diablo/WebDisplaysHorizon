//! How a block treats sound: how much it sends back, how much it lets through.
//!
//! The numbers are Sound Physics Remastered's defaults. That mod keys them on the block's
//! sound type, which Horizon does not tell a client mod; here the block's name stands in
//! for it, which sorts the vanilla blocks the same way and gives a modded block a sensible
//! middle value.

/// What a block does to sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// How much of a sound it sends back: 0 swallows it, 1.5 is bare stone.
    pub reflectivity: f32,
    /// How much it muffles what goes through it: 1 for an ordinary block.
    pub occlusion: f32,
    /// A ray bounces off it. False for a block too thin or too open to be a wall (a fence,
    /// a torch, a slab's worth of carpet): sound goes round it and loses a little.
    pub wall: bool,
}

/// A block that is less than a whole cube muffles this much of what a whole one would.
const PARTIAL: f32 = 0.25;

const STONE: (f32, f32) = (1.5, 1.0);
const METAL: (f32, f32) = (1.25, 1.0);
const WOOD: (f32, f32) = (0.4, 1.0);
const GLASS: (f32, f32) = (0.75, 0.1);
const WOOL: (f32, f32) = (0.1, 1.5);
const EARTH: (f32, f32) = (0.3, 1.0);
const SAND: (f32, f32) = (0.2, 1.0);
const SNOW: (f32, f32) = (0.15, 0.1);
const LEAVES: (f32, f32) = (0.2, 0.25);
const MOSS: (f32, f32) = (0.1, 0.75);
const HONEY: (f32, f32) = (0.1, 0.5);
const OTHER: (f32, f32) = (0.5, 1.0);

/// Reflectivity and occlusion by the words in a block's name, the first match winning.
/// Order matters: `sandstone` is stone before `sand` is sand, and `mossy_stone_bricks` is
/// stone before `moss_block` is moss.
const WORDS: [(&[&str], (f32, f32)); 12] = [
    (&["wool", "carpet", "sponge", "_bed", "hay_block"], WOOL),
    (&["glass", "ice", "webdisplays:screen"], GLASS),
    (&["leaves", "azalea", "vine", "cobweb"], LEAVES),
    (&["honey", "slime"], HONEY),
    (&["snow"], SNOW),
    (
        &[
            "stone", "brick", "deepslate", "_ore", "obsidian", "concrete", "terracotta", "andesite", "diorite", "granite",
            "basalt", "tuff", "calcite", "quartz", "prismarine", "purpur", "bedrock", "amethyst", "bone_block", "netherrack",
            "cobble", "furnace", "smoker", "dispenser", "dropper", "observer", "piston",
        ],
        STONE,
    ),
    (&["moss"], MOSS),
    (&["iron", "gold", "copper", "netherite", "anvil", "chain", "cauldron", "hopper", "lantern", "bell", "speaker"], METAL),
    (
        &[
            "log", "planks", "wood", "_stem", "hyphae", "door", "fence", "bookshelf", "chest", "barrel", "crafting_table",
            "sign", "bamboo", "ladder", "loom", "lectern", "note_block", "jukebox", "composter", "beehive",
        ],
        WOOD,
    ),
    (&["soul_sand", "soul_soil", "sand", "powder"], SAND),
    (&["dirt", "grass_block", "podzol", "mycelium", "farmland", "mud", "clay", "gravel", "path"], EARTH),
    (&[], OTHER),
];

/// What the block named `name` (`minecraft:oak_planks`) does to sound. `collides` and
/// `whole` are the game's facts about its state: whether anything can bump into it, and
/// whether it fills its cube. None for a block sound ignores: air, flowers, water.
pub fn of(name: &str, collides: bool, whole: bool) -> Option<Material> {
    if !collides {
        return None;
    }
    // concrete_powder is a powder, not concrete.
    let (reflectivity, occlusion) = if name.contains("concrete_powder") {
        SAND
    } else {
        WORDS.iter().find(|(words, _)| words.is_empty() || words.iter().any(|word| name.contains(word))).map_or(OTHER, |(_, values)| *values)
    };
    Some(Material { reflectivity, occlusion: if whole { occlusion } else { occlusion * PARTIAL }, wall: whole })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn whole(name: &str) -> Material {
        of(name, true, true).unwrap()
    }

    #[test]
    fn stone_rings_wood_less_and_wool_swallows() {
        let reflectivity = |name: &str| whole(name).reflectivity;
        assert_eq!(reflectivity("minecraft:stone"), 1.5);
        assert_eq!(reflectivity("minecraft:iron_block"), 1.25);
        assert_eq!(reflectivity("minecraft:oak_planks"), 0.4);
        assert_eq!(reflectivity("minecraft:white_wool"), 0.1);
        assert!(reflectivity("minecraft:stone") > reflectivity("minecraft:glass"));
        assert!(reflectivity("minecraft:glass") > reflectivity("minecraft:dirt"));
    }

    #[test]
    fn a_name_with_two_words_takes_the_harder_reading() {
        assert_eq!(whole("minecraft:sandstone"), whole("minecraft:stone"));
        assert_eq!(whole("minecraft:mossy_stone_bricks").reflectivity, 1.5, "stone with moss on it is stone");
        assert_eq!(whole("minecraft:moss_block").reflectivity, 0.1);
        assert_eq!(whole("minecraft:red_concrete_powder").reflectivity, 0.2);
        assert_eq!(whole("minecraft:red_concrete").reflectivity, 1.5);
        assert_eq!(whole("somemod:mystery_machine").reflectivity, 0.5, "an unknown block is middling");
    }

    #[test]
    fn wool_muffles_most_and_glass_hardly() {
        assert_eq!(whole("minecraft:white_wool").occlusion, 1.5);
        assert_eq!(whole("minecraft:glass").occlusion, 0.1);
        assert_eq!(whole("minecraft:stone").occlusion, 1.0);
    }

    #[test]
    fn what_nothing_bumps_into_is_not_there_and_a_part_block_is_no_wall() {
        assert_eq!(of("minecraft:air", false, false), None);
        assert_eq!(of("minecraft:poppy", false, false), None);
        let fence = of("minecraft:oak_fence", true, false).unwrap();
        assert!(!fence.wall);
        assert_eq!(fence.occlusion, 0.25);
        assert!(whole("minecraft:oak_planks").wall);
    }
}
