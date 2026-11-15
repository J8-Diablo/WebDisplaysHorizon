//! How many pixels a screen's browser draws.
//!
//! A screen saves one resolution, the same for every player. Each client multiplies it by
//! its own quality scale to get what its browser really renders (the "effective"
//! resolution). The server's maximum and the client's quality scale are arguments here,
//! never globals: both ends and the tests call the same code.
//!
//! A resolution is always written width along the side's right, height along its up,
//! whatever the screen's rotation. A quarter turn only swaps the two numbers the browser is
//! given (`browser_size`).
//!
//! Shapes are fitted to the screen's native size, 16 pixels a block minus the 4 the bezel
//! hides, not to its block count: a 2x1 screen is 28:12, not 2:1.
//!
//! The arithmetic (single precision, where it rounds and where it truncates) is the Java
//! mod's, so both give the same numbers. Two things differ on purpose: a maximum of zero
//! counts as one, and no result has a zero side (the Java mod could shrink a very thin
//! resolution to a height of zero).

use crate::model::side::Rotation;

/// What a new screen aims for, however high the configured maximum is: full HD.
pub const DEFAULT_TARGET: [u32; 2] = [1920, 1080];

/// A screen's size in pixels at the game's own density: 16 a block, minus the 2 the bezel
/// hides on each edge. Never below one pixel.
pub fn native_size(blocks: [u32; 2]) -> [f32; 2] {
    blocks.map(|side| (side as f32 * 16.0 - 4.0).max(1.0))
}

/// The width over the height of a screen's picture. Not its block ratio: the bezel makes a
/// small screen squarer than its block count says.
pub fn native_aspect(blocks: [u32; 2]) -> f32 {
    let [width, height] = native_size(blocks);
    width / height
}

/// The largest resolution of the screen's own shape that fits in `target`: one side reaches
/// the box, the other follows the shape.
pub fn fit_in_box(blocks: [u32; 2], target: [u32; 2]) -> [u32; 2] {
    let native = native_size(blocks);
    let scale = (target[0].max(1) as f32 / native[0]).min(target[1].max(1) as f32 / native[1]);
    native.map(|side| ((side * scale).round() as u32).max(1))
}

/// The resolution of a newly made screen: its shape fitted in full HD, or in the configured
/// maximum where that is smaller. A higher maximum is for players to opt into per screen.
pub fn default_resolution(blocks: [u32; 2], max: [u32; 2]) -> [u32; 2] {
    fit_in_box(blocks, [DEFAULT_TARGET[0].min(max[0]), DEFAULT_TARGET[1].min(max[1])])
}

/// The resolution of a chosen width: the height follows the screen's shape. Both stay
/// within the maximum; when the height would pass it, the width gives way instead.
pub fn fit_to_shape(blocks: [u32; 2], target_width: u32, max: [u32; 2]) -> [u32; 2] {
    let native = native_size(blocks);
    let max = at_least_one(max);
    let mut width = target_width.min(max[0]).max(1);
    let mut height = ((width as f32 * (native[1] / native[0])).round() as u32).max(1);
    if height > max[1] {
        height = max[1];
        width = ((height as f32 * (native[0] / native[1])).round() as u32).max(1);
    }
    [width, height]
}

/// A resolution brought within the maximum, keeping its shape: first the width, then the
/// height, the other side shrunk by the same ratio and truncated.
pub fn clamp(resolution: [u32; 2], max: [u32; 2]) -> [u32; 2] {
    at_least_one(shrink(resolution, at_least_one(max)))
}

/// What a client's browser renders for a screen of this saved resolution: scaled by the
/// client's quality (a percentage, 1 to 100, rounded to the nearest pixel), then brought
/// within the maximum. Still width along the side's right: see `browser_size`.
pub fn effective(resolution: [u32; 2], quality_percent: u32, max: [u32; 2]) -> [u32; 2] {
    let scale = quality_percent.clamp(1, 100) as f64 / 100.0;
    clamp(resolution.map(|side| ((side as f64 * scale).round() as u32).max(1)), max)
}

/// The width and height to give the browser: a quarter turn swaps them, since the page's
/// width then runs along the screen's height.
pub fn browser_size(effective: [u32; 2], rotation: Rotation) -> [u32; 2] {
    let [width, height] = at_least_one(effective);
    if rotation.is_vertical() { [height, width] } else { [width, height] }
}

/// Whether a saved resolution is what old versions gave every new screen, eight times the
/// native size brought within the maximum: then no player chose it.
pub fn is_legacy_auto(blocks: [u32; 2], resolution: [u32; 2], max: [u32; 2]) -> bool {
    if blocks[0] == 0 || blocks[1] == 0 {
        return false;
    }
    let legacy = blocks.map(|side| ((side as f32 * 16.0 - 4.0) * 8.0) as u32);
    resolution == shrink(legacy, at_least_one(max))
}

/// The resolution of a screen read from a save: the saved one, unless it is missing,
/// invalid or the old automatic default, in which case the screen gets today's default.
/// A resolution a player chose is kept as it is, even above the maximum.
pub fn restored(blocks: [u32; 2], saved: [i32; 2], max: [u32; 2]) -> [u32; 2] {
    match saved.map(u32::try_from) {
        [Ok(width), Ok(height)]
            if width > 0 && height > 0 && !is_legacy_auto(blocks, [width, height], max) =>
        {
            [width, height]
        }
        _ => default_resolution(blocks, max),
    }
}

/// The shrinking behind `clamp`, exactly as old saves were made with it: a side may reach
/// zero.
fn shrink(resolution: [u32; 2], max: [u32; 2]) -> [u32; 2] {
    let [mut width, mut height] = resolution;
    if width > max[0] {
        height = (height as f32 * max[0] as f32 / width as f32) as u32;
        width = max[0];
    }
    if height > max[1] {
        width = (width as f32 * max[1] as f32 / height as f32) as u32;
        height = max[1];
    }
    [width, height]
}

fn at_least_one(pair: [u32; 2]) -> [u32; 2] {
    pair.map(|side| side.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_HD: [u32; 2] = [1920, 1080];
    const UHD: [u32; 2] = [3840, 2160];

    #[test]
    fn the_native_size_is_sixteen_pixels_a_block_minus_the_bezel() {
        assert_eq!(native_size([1, 1]), [12.0, 12.0]);
        assert_eq!(native_size([16, 9]), [252.0, 140.0]);
        assert_eq!(native_size([0, 3]), [1.0, 44.0], "never below a pixel");
        assert_eq!(native_aspect([2, 1]), 28.0 / 12.0, "squarer than its two blocks by one");
    }

    #[test]
    fn a_new_screen_fills_full_hd_without_changing_shape() {
        // Square screens are stopped by the height.
        assert_eq!(default_resolution([1, 1], FULL_HD), [1080, 1080]);
        assert_eq!(default_resolution([16, 16], FULL_HD), [1080, 1080]);
        // 60x12 native: the width reaches 1920 first, 12 * 32 = 384.
        assert_eq!(default_resolution([4, 1], FULL_HD), [1920, 384]);
        // 12x60 native: the height reaches 1080 first, 12 * 18 = 216.
        assert_eq!(default_resolution([1, 4], FULL_HD), [216, 1080]);
        // 12x252 native: 12 * 1080 / 252 = 51.4.
        assert_eq!(default_resolution([1, 16], FULL_HD), [51, 1080]);
        // 252x140 native is a little wider than 16:9: 140 * 1920 / 252 = 1066.7.
        assert_eq!(default_resolution([16, 9], FULL_HD), [1920, 1067]);
        // 60x44 native: 60 * 1080 / 44 = 1472.7.
        assert_eq!(default_resolution([4, 3], FULL_HD), [1473, 1080]);
        // 28x12 native: 12 * 1920 / 28 = 822.9.
        assert_eq!(default_resolution([2, 1], FULL_HD), [1920, 823]);
    }

    #[test]
    fn the_default_passes_neither_the_configured_maximum_nor_full_hd() {
        assert_eq!(default_resolution([16, 9], [1280, 720]), [1280, 711]);
        assert_eq!(default_resolution([16, 9], UHD), [1920, 1067], "a higher maximum is opt-in");
        assert_eq!(default_resolution([16, 9], [3840, 720]), [1296, 720], "each side on its own");
        assert_eq!(default_resolution([4, 4], [0, 0]), [1, 1], "a maximum of zero counts as one");
    }

    #[test]
    fn a_resolution_fitted_in_a_box_keeps_the_shape_and_touches_one_side() {
        assert_eq!(fit_in_box([4, 3], [600, 600]), [600, 440]);
        assert_eq!(fit_in_box([1, 4], [600, 600]), [120, 600]);
        assert_eq!(fit_in_box([4, 3], [0, 0]), [1, 1]);
    }

    #[test]
    fn the_height_follows_the_shape_from_a_chosen_width() {
        // 1280 * 140 / 252 = 711.1.
        assert_eq!(fit_to_shape([16, 9], 1280, FULL_HD), [1280, 711]);
        assert_eq!(fit_to_shape([16, 9], 5000, FULL_HD), [1920, 1067], "the width is capped first");
        // 1920 * 44 / 60 = 1408 is too tall: the height stops at 1080 and the width gives way.
        assert_eq!(fit_to_shape([4, 3], 1920, FULL_HD), [1473, 1080]);
        assert_eq!(fit_to_shape([4, 3], 1920, UHD), [1920, 1408]);
        assert_eq!(fit_to_shape([16, 9], 0, FULL_HD), [1, 1], "never zero");
        assert_eq!(fit_to_shape([1, 16], 1, FULL_HD), [1, 21], "12x252 native");
    }

    #[test]
    fn clamping_keeps_the_shape_and_truncates() {
        assert_eq!(clamp([1280, 720], FULL_HD), [1280, 720], "already within");
        assert_eq!(clamp([3840, 2160], FULL_HD), [1920, 1080]);
        assert_eq!(clamp([4000, 1000], FULL_HD), [1920, 480]);
        assert_eq!(clamp([1000, 4000], FULL_HD), [270, 1080]);
        // 1120 * 1920 / 2016 = 1066.7: truncated, where a fit would round to 1067.
        assert_eq!(clamp([2016, 1120], FULL_HD), [1920, 1066]);
        // Both steps: the width first (1920x1920), then the height.
        assert_eq!(clamp([4000, 4000], FULL_HD), [1080, 1080]);
        assert_eq!(clamp([4000, 1], FULL_HD), [1920, 1], "a side never shrinks to nothing");
    }

    #[test]
    fn the_quality_scale_rounds_to_the_nearest_pixel() {
        assert_eq!(effective([1920, 1067], 100, FULL_HD), [1920, 1067]);
        assert_eq!(effective([1920, 1067], 50, FULL_HD), [960, 534], "533.5 rounds up");
        assert_eq!(effective([1920, 1067], 33, FULL_HD), [634, 352], "633.6 and 352.1");
        assert_eq!(effective([1920, 1067], 1, FULL_HD), [19, 11], "19.2 and 10.7");
        assert_eq!(effective([30, 30], 1, FULL_HD), [1, 1], "0.3 of a pixel is still one");
    }

    #[test]
    fn a_quality_outside_one_to_a_hundred_is_brought_back_in() {
        assert_eq!(effective([1000, 500], 0, FULL_HD), [10, 5]);
        assert_eq!(effective([1000, 500], 250, FULL_HD), [1000, 500]);
    }

    #[test]
    fn the_effective_resolution_is_capped_by_the_maximum() {
        assert_eq!(effective([3840, 2160], 100, FULL_HD), [1920, 1080]);
        assert_eq!(effective([3840, 2160], 75, FULL_HD), [1920, 1080], "2880x1620 before the cap");
        assert_eq!(effective([3840, 2160], 25, FULL_HD), [960, 540], "the scale alone is enough");
        assert_eq!(effective([3840, 2160], 100, UHD), [3840, 2160]);
    }

    #[test]
    fn a_quarter_turn_swaps_the_sides_the_browser_is_given() {
        assert_eq!(browser_size([1920, 1067], Rotation::R0), [1920, 1067]);
        assert_eq!(browser_size([1920, 1067], Rotation::R180), [1920, 1067]);
        assert_eq!(browser_size([1920, 1067], Rotation::R90), [1067, 1920]);
        assert_eq!(browser_size([1920, 1067], Rotation::R270), [1067, 1920]);
        assert_eq!(browser_size([0, 0], Rotation::R90), [1, 1], "a browser has at least a pixel");
    }

    #[test]
    fn a_saved_resolution_equal_to_the_old_default_is_recognised() {
        // (4 * 16 - 4) * 8 by (3 * 16 - 4) * 8.
        assert!(is_legacy_auto([4, 3], [480, 352], FULL_HD));
        assert!(!is_legacy_auto([4, 3], [480, 353], FULL_HD));
        assert!(!is_legacy_auto([3, 4], [480, 352], FULL_HD), "the default of another shape");
        // 2016x1120 was brought within the maximum of its day, truncating.
        assert!(is_legacy_auto([16, 9], [1920, 1066], FULL_HD));
        assert!(!is_legacy_auto([16, 9], [1920, 1066], UHD), "under this maximum it was 2016x1120");
        assert!(is_legacy_auto([16, 9], [2016, 1120], UHD));
        // 96x2016: the height stops at 1080, 96 * 1080 / 2016 = 51.4.
        assert!(is_legacy_auto([1, 16], [51, 1080], FULL_HD));
        assert!(!is_legacy_auto([0, 3], [0, 352], FULL_HD), "no blocks, no default");
    }

    #[test]
    fn a_save_with_the_old_default_gets_the_new_one() {
        assert_eq!(restored([4, 3], [480, 352], FULL_HD), [1473, 1080]);
        assert_eq!(restored([16, 9], [1920, 1066], FULL_HD), [1920, 1067]);
        assert_eq!(restored([1, 1], [96, 96], FULL_HD), [1080, 1080]);
    }

    #[test]
    fn a_save_with_no_valid_resolution_gets_the_default_and_a_chosen_one_is_kept() {
        assert_eq!(restored([4, 3], [0, 0], FULL_HD), [1473, 1080], "missing");
        assert_eq!(restored([4, 3], [1280, 0], FULL_HD), [1473, 1080]);
        assert_eq!(restored([4, 3], [-5, 720], FULL_HD), [1473, 1080]);
        assert_eq!(restored([4, 3], [1280, 720], FULL_HD), [1280, 720], "a player's choice");
        assert_eq!(restored([4, 3], [1473, 1080], FULL_HD), [1473, 1080]);
        assert_eq!(restored([4, 3], [3840, 2160], FULL_HD), [3840, 2160], "reading does not clamp");
    }
}
