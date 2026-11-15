//! Which browser pixel a point of a screen shows, and where on the screen a pixel is.
//!
//! A hit is a point in blocks, measured from the lowest corner of the screen's origin block:
//! the block at the screen's lower left, as someone facing the screen sees it. On its way to
//! a pixel it is expressed in four frames:
//!
//! 1. the face: blocks along the side's right and up, from the screen's lower-left corner;
//! 2. the drawn area: the same with the bezel taken off, 0 to 1 each way;
//! 3. the picture: 0 to 1 from the page's top-left corner, the screen's rotation undone;
//! 4. pixels of the browser.
//!
//! Things that surprise:
//!
//! - A resolution is always width along the side's right, height along its up. With a
//!   quarter turn the browser gets the two swapped, so a pixel's x is then counted against
//!   the resolution's second number.
//! - On the BOTTOM side the page's top is at the side's "down" (south): seen from below,
//!   east stays on the right only that way round. So Y is flipped on every side but that one.
//! - `hit_to_pixel` counts in the screen's saved resolution, the one number every client
//!   agrees on: that is what travels. Each client then turns it into a pixel of what it
//!   really shows: its own browser at its quality (`to_effective`), a shared browser's
//!   letterboxed picture (`to_shared`), or a linked group's (`to_group`).
//! - A linked group's picture is laid over the group's whole rectangle exactly as a lone
//!   screen's is over its own: turned as one piece, top at the side's up.
//!
//! Differs from the Java mod on purpose:
//!
//! - A pixel is always inside the picture. The Java mod gave a hit on the far inner edge
//!   of the bezel a pixel one past the last, and so did its scaling of the last pixels.
//! - `pixel_to_hit` gives a point in the frame `hit_to_pixel` takes. The Java mod's
//!   keyboard camera got its x one block further along on the sides whose right is
//!   positive, its y one block less on the floor and the ceiling, and undid both later.
//! - `to_group` goes through the face. The Java mod added the page's Y, counted down, to
//!   an offset counted up, so clicks on stacked screens landed in the wrong screen's part;
//!   it also turned each screen of a turned group on its own instead of the group.

use crate::model::letterbox::{BEZEL, Fit};
use crate::model::side::{Rotation, Side};

/// What the conversions need to know of a screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Face {
    /// The face of the blocks the screen is on.
    pub side: Side,
    /// How its picture is turned on that face.
    pub rotation: Rotation,
    /// Its size in blocks: along the side's right, then along its up.
    pub size: [u32; 2],
}

impl Face {
    /// The width and height of what is drawn, in blocks: the bezel is off each edge.
    pub fn drawn(&self) -> [f32; 2] {
        self.size.map(|side| side as f32 - 2.0 * BEZEL)
    }
}

/// The part of a linked group's picture one screen shows. Measured on the face, in blocks
/// along the side's right and up, whatever the rotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tile {
    /// Where the screen's lower-left corner is in the group.
    pub offset: [u32; 2],
    /// The group's size.
    pub group: [u32; 2],
}

impl Tile {
    /// A point of the screen, in blocks from its lower-left corner, as a point of the
    /// group's rectangle: 0 to 1 along the side's right and up.
    pub fn at(&self, blocks: [f32; 2]) -> [f32; 2] {
        [0, 1].map(|i| (self.offset[i] as f32 + blocks[i]) / self.group[i].max(1) as f32)
    }

    /// A point of the drawn area of a screen of `size` blocks as a point of the group's
    /// rectangle. The bezel takes none of the picture: the screen's whole part of it is
    /// squeezed into what is drawn.
    pub fn to_group(&self, size: [u32; 2], point: [f32; 2]) -> [f32; 2] {
        self.at([0, 1].map(|i| point[i] * size[i] as f32))
    }

    /// The reverse of `to_group`. Outside 0 to 1 when another screen of the group shows
    /// that point, or none does.
    pub fn to_screen(&self, size: [u32; 2], point: [f32; 2]) -> [f32; 2] {
        [0, 1].map(|i| (point[i] * self.group[i] as f32 - self.offset[i] as f32) / size[i].max(1) as f32)
    }
}

/// A hit as a point of the face: blocks along the side's right and up from the screen's
/// lower-left corner, bezel included. How deep the hit is does not count.
pub fn face_point(side: Side, hit: [f32; 3]) -> [f32; 2] {
    let (right, up) = (side.right(), side.up());
    let along = |axis: [i32; 3]| (0..3).map(|i| from_edge(axis[i], hit[i])).sum::<f32>();
    [along(right), along(up)]
}

/// The reverse of `face_point`: the point of the face as a hit, on the surface of the
/// origin block that looks out of the screen.
pub fn block_point(side: Side, point: [f32; 2]) -> [f32; 3] {
    let (right, up, forward) = (side.right(), side.up(), side.forward());
    [0, 1, 2].map(|i| {
        let surface = if forward[i] > 0 { 1.0 } else { 0.0 };
        from_edge(right[i], point[0]) + from_edge(up[i], point[1]) + surface
    })
}

/// The face of the block at `pos` a hit at `point` (world coordinates) landed on: the one
/// whose plane the point is in. None for a point on none of them. On an edge, the first
/// side in `Side::ALL` order wins.
pub fn hit_side(pos: [i32; 3], point: [f64; 3]) -> Option<Side> {
    Side::ALL.into_iter().find(|side| {
        let forward = side.forward();
        (0..3).any(|i| forward[i] != 0 && (point[i] - f64::from(pos[i]) - f64::from(forward[i].max(0))).abs() < 1e-3)
    })
}

/// One coordinate against one component of an axis. An axis that runs backwards starts
/// from the block's far edge, which is at 1. Its own reverse.
fn from_edge(way: i32, value: f32) -> f32 {
    match way.signum() {
        1 => value,
        -1 => 1.0 - value,
        _ => 0.0,
    }
}

/// A point of the drawn area (0 to 1 along the side's right and up) as a point of the
/// picture (0 to 1 from the page's left and from its top).
pub fn face_to_picture(side: Side, rotation: Rotation, point: [f32; 2]) -> [f32; 2] {
    let [x, y] = point;
    let y = if side == Side::Bottom { y } else { 1.0 - y };
    let [x, y] = quarter_turns(rotation, [x, y]);
    if rotation.is_vertical() { [y, x] } else { [x, y] }
}

/// The reverse of `face_to_picture`.
pub fn picture_to_face(side: Side, rotation: Rotation, point: [f32; 2]) -> [f32; 2] {
    let [x, y] = if rotation.is_vertical() { [point[1], point[0]] } else { point };
    let [x, y] = quarter_turns(rotation, [x, y]);
    [x, if side == Side::Bottom { y } else { 1.0 - y }]
}

/// The mirrorings a rotation is made of, the swap of the two axes aside. Its own reverse.
fn quarter_turns(rotation: Rotation, [x, y]: [f32; 2]) -> [f32; 2] {
    match rotation {
        Rotation::R0 => [x, y],
        Rotation::R90 => [x, 1.0 - y],
        Rotation::R180 => [1.0 - x, 1.0 - y],
        Rotation::R270 => [1.0 - x, y],
    }
}

/// A resolution as the browser has it: its two sides swapped by a quarter turn.
fn turned(resolution: [u32; 2], rotation: Rotation) -> [u32; 2] {
    if rotation.is_vertical() { [resolution[1], resolution[0]] } else { resolution }
}

/// The pixel a hit on a screen shows, counted in `resolution` (the screen's saved one).
/// None when the hit is on the bezel, off the screen, or where the screen's shape has no
/// block.
///
/// `inside` is the shape's test. It is asked in the drawn area's frame: 0 to 1 along the
/// side's right, then along its up, before any flip or rotation. Multiplied by the size in
/// blocks, that is the cell of the shape under the point.
pub fn hit_to_pixel(
    face: &Face,
    resolution: [u32; 2],
    hit: [f32; 3],
    inside: impl Fn(f32, f32) -> bool,
) -> Option<[u32; 2]> {
    let on_face = face_point(face.side, hit);
    let drawn = face.drawn();
    let [x, y] = [0, 1].map(|i| (on_face[i] - BEZEL) / drawn[i]);
    // Written so that NaN is outside too.
    if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) || !inside(x, y) {
        return None;
    }
    let picture = face_to_picture(face.side, face.rotation, [x, y]);
    let count = turned(resolution, face.rotation);
    Some([0, 1].map(|i| ((picture[i] * count[i] as f32) as u32).min(count[i].saturating_sub(1))))
}

/// Where a pixel is on a screen: a hit, in the frame `hit_to_pixel` takes.
///
/// `resolution` is that of the picture the pixel is counted in, saved or effective alike:
/// the screen's own, or the group's when the screen is linked and `tile` says which part of
/// the group's picture it shows. A group's pixel shown by another screen gives a point off
/// this one, on that side.
pub fn pixel_to_hit(face: &Face, tile: Option<&Tile>, resolution: [u32; 2], pixel: [f32; 2]) -> [f32; 3] {
    let count = turned(resolution, face.rotation);
    let picture = [0, 1].map(|i| pixel[i] / count[i].max(1) as f32);
    let mut point = picture_to_face(face.side, face.rotation, picture);
    if let Some(tile) = tile {
        point = tile.to_screen(face.size, point);
    }
    let drawn = face.drawn();
    block_point(face.side, [0, 1].map(|i| point[i] * drawn[i] + BEZEL))
}

/// A pixel counted in a screen's saved `resolution` as a pixel of what this client's
/// browser renders (`effective`, see `resolution::effective`). Rounded to the nearest.
/// Serves a group as well, with the group's two resolutions.
pub fn to_effective(
    rotation: Rotation,
    resolution: [u32; 2],
    effective: [u32; 2],
    pixel: [u32; 2],
) -> [u32; 2] {
    let from = turned(resolution, rotation);
    let to = turned(effective, rotation);
    if from[0] == 0 || from[1] == 0 || from == to {
        return pixel;
    }
    [0, 1].map(|i| {
        let scaled = (pixel[i] as f64 * (to[i] as f64 / from[i] as f64)).round() as u32;
        scaled.min(to[i].saturating_sub(1))
    })
}

/// A pixel counted in a screen's saved `resolution` as a pixel of the shared browser the
/// screen shows, whose picture is `shared` pixels and letterboxed on the screen. None on a
/// bar: a click there is nobody's, and is not pushed to the picture's edge.
pub fn to_shared(face: &Face, resolution: [u32; 2], shared: [u32; 2], pixel: [u32; 2]) -> Option<[u32; 2]> {
    let count = turned(resolution, face.rotation);
    if shared[0] == 0 || shared[1] == 0 || count[0] == 0 || count[1] == 0 {
        return None;
    }
    let fit = Fit::for_screen(face.size, face.rotation.is_vertical(), shared);
    let on_picture = fit.to_content([0, 1].map(|i| pixel[i] as f32 / count[i] as f32))?;
    Some([0, 1].map(|i| ((on_picture[i] * shared[i] as f32).round() as u32).min(shared[i] - 1)))
}

/// A pixel counted in a linked screen's saved `resolution` as a pixel of its group's
/// picture, counted in `group_resolution`: the group's saved one (then `to_effective` gives
/// the browser's), or its effective one directly.
pub fn to_group(
    face: &Face,
    resolution: [u32; 2],
    tile: &Tile,
    group_resolution: [u32; 2],
    pixel: [u32; 2],
) -> [u32; 2] {
    let own = turned(resolution, face.rotation);
    let whole = turned(group_resolution, face.rotation);
    let on_picture = [0, 1].map(|i| pixel[i] as f32 / own[i].max(1) as f32);
    let on_screen = picture_to_face(face.side, face.rotation, on_picture);
    let on_group = face_to_picture(face.side, face.rotation, tile.to_group(face.size, on_screen));
    [0, 1].map(|i| ((on_group[i] * whole[i] as f32).round() as u32).min(whole[i].saturating_sub(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hit_names_the_face_it_landed_on() {
        let pos = [-31, 68, 6];
        assert_eq!(hit_side(pos, [-30.4, 68.5, 6.0]), Some(Side::North));
        assert_eq!(hit_side(pos, [-30.4, 68.5, 7.0]), Some(Side::South));
        assert_eq!(hit_side(pos, [-31.0, 68.5, 6.5]), Some(Side::West));
        assert_eq!(hit_side(pos, [-30.0, 68.5, 6.5]), Some(Side::East));
        assert_eq!(hit_side(pos, [-30.5, 69.0, 6.5]), Some(Side::Top));
        assert_eq!(hit_side(pos, [-30.5, 68.0, 6.5]), Some(Side::Bottom));
        assert_eq!(hit_side(pos, [-30.5, 68.5, 6.5]), None, "inside the block");
    }
    use std::cell::Cell;

    /// A 4x2 screen at a thousand pixels a block of drawn area: pixels read as blocks.
    const RESOLUTION: [u32; 2] = [3750, 1750];

    fn face(side: Side, rotation: Rotation) -> Face {
        Face { side, rotation, size: [4, 2] }
    }

    fn anywhere(_: f32, _: f32) -> bool {
        true
    }

    /// The hit at `x` blocks along the side's right and `y` along its up from the screen's
    /// lower-left corner, written out side by side from where each face of the origin block
    /// is in the world, without the code under test.
    fn hit_at(side: Side, [x, y]: [f32; 2]) -> [f32; 3] {
        match side {
            // Looking north at the block's south face: east is right.
            Side::South => [x, y, 1.0],
            // Looking south: west is right, and the block's left edge is its east one.
            Side::North => [1.0 - x, y, 0.0],
            // Looking east at the west face: south is right.
            Side::West => [0.0, y, x],
            // Looking west: north is right.
            Side::East => [1.0, y, 1.0 - x],
            // East is right and north is up, from above as from below.
            Side::Top => [x, 1.0, 1.0 - y],
            Side::Bottom => [x, 0.0, 1.0 - y],
        }
    }

    fn apart(a: [f32; 3], b: [f32; 3]) -> f32 {
        (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0, f32::max)
    }

    #[test]
    fn a_hit_gives_the_pixel_under_it_on_every_side() {
        // Just over one block right and half a block up of the drawn area's corner.
        let point = [BEZEL + 1.0005, BEZEL + 0.5005];
        for side in Side::ALL {
            let pixel = hit_to_pixel(&face(side, Rotation::R0), RESOLUTION, hit_at(side, point), anywhere);
            // Half a block up is 1.25 blocks down from the top of the 1.75 drawn.
            let expected = if side == Side::Bottom { [1000, 500] } else { [1000, 1249] };
            assert_eq!(pixel, Some(expected), "{side:?}");
        }
    }

    #[test]
    fn the_bottom_side_keeps_its_y_unflipped() {
        // Near the side's up edge, which is north on the floor and on the ceiling.
        let point = [BEZEL + 2.0005, BEZEL + 1.7005];
        let on = |side| hit_to_pixel(&face(side, Rotation::R0), RESOLUTION, hit_at(side, point), anywhere);
        assert_eq!(on(Side::Top), Some([2000, 49]), "the page's top is north on the ceiling's upper face");
        assert_eq!(on(Side::Bottom), Some([2000, 1700]), "and south seen from below");
    }

    #[test]
    fn each_rotation_turns_the_picture_a_quarter_counter_clockwise() {
        // 0.75 blocks from the left and 1.5 from the bottom of the drawn area: upper left.
        let hit = hit_at(Side::South, [BEZEL + 0.7505, BEZEL + 1.5005]);
        let on = |rotation| hit_to_pixel(&face(Side::South, rotation), RESOLUTION, hit, anywhere);
        assert_eq!(on(Rotation::R0), Some([750, 249]));
        // The page's top is now at the screen's left and its right at the screen's top: the
        // browser is 1750 wide and 3750 tall.
        assert_eq!(on(Rotation::R90), Some([1500, 750]));
        assert_eq!(on(Rotation::R180), Some([2999, 1500]), "upside down: 3750 - 750.5, 1750 - 249.5");
        assert_eq!(on(Rotation::R270), Some([249, 2999]), "the page's top is at the screen's right");
    }

    #[test]
    fn the_bezel_is_not_part_of_the_picture_and_its_inner_edges_are() {
        let face = face(Side::South, Rotation::R0);
        let on = |x: f32, y: f32| hit_to_pixel(&face, RESOLUTION, hit_at(Side::South, [x, y]), anywhere);
        assert_eq!(on(BEZEL, BEZEL), Some([0, 1749]), "lower-left corner: the last row, not one past");
        assert_eq!(on(4.0 - BEZEL, 2.0 - BEZEL), Some([3749, 0]), "upper-right corner: the last column");
        assert_eq!(on(BEZEL - 0.001, 1.0), None, "left bezel");
        assert_eq!(on(4.0 - BEZEL + 0.001, 1.0), None, "right bezel");
        assert_eq!(on(2.0, BEZEL - 0.001), None, "bottom bezel");
        assert_eq!(on(2.0, 2.0 - BEZEL + 0.001), None, "top bezel");
        assert_eq!(on(-3.0, 1.0), None, "off the screen");
        assert_eq!(on(f32::NAN, 1.0), None);
    }

    #[test]
    fn a_point_outside_the_shape_hits_nothing() {
        let face = face(Side::South, Rotation::R0);
        let left_half = |x: f32, _: f32| x < 0.5;
        let on = |x: f32| hit_to_pixel(&face, RESOLUTION, hit_at(Side::South, [x, 1.0]), left_half);
        assert!(on(1.0).is_some());
        assert_eq!(on(3.0), None);
    }

    #[test]
    fn the_shape_is_asked_along_the_sides_right_and_up_whatever_the_side_and_rotation() {
        for side in Side::ALL {
            for rotation in Rotation::ALL {
                let face = face(side, rotation);
                let drawn = face.drawn();
                let asked = Cell::new([f32::NAN; 2]);
                let hit = hit_at(side, [BEZEL + drawn[0] * 0.25, BEZEL + drawn[1] * 0.75]);
                let pixel = hit_to_pixel(&face, RESOLUTION, hit, |x, y| {
                    asked.set([x, y]);
                    true
                });
                assert!(pixel.is_some());
                let [x, y] = asked.get();
                assert!((x - 0.25).abs() < 1e-5 && (y - 0.75).abs() < 1e-5, "{side:?} {rotation:?}: {x} {y}");
            }
        }
    }

    #[test]
    fn a_pixel_gives_back_its_point_on_the_face() {
        // One block right of the drawn area's left edge, a quarter block under its top.
        let pixel = [1000.0, 250.0];
        let at = |side| pixel_to_hit(&face(side, Rotation::R0), None, RESOLUTION, pixel);
        assert!(apart(at(Side::South), [1.125, 1.625, 1.0]) < 1e-5);
        assert!(apart(at(Side::North), [-0.125, 1.625, 0.0]) < 1e-5, "its right is west");
        assert!(apart(at(Side::East), [1.0, 1.625, -0.125]) < 1e-5, "its right is north");
        assert!(apart(at(Side::Top), [1.125, 1.0, -0.625]) < 1e-5, "its up is north");
        assert!(apart(at(Side::Bottom), [1.125, 0.0, 0.625]) < 1e-5, "the page's top is south");
    }

    #[test]
    fn face_points_and_hits_are_each_others_reverse() {
        for side in Side::ALL {
            let point = [2.75, 1.25];
            let hit = block_point(side, point);
            assert!(apart(hit, hit_at(side, point)) < 1e-6, "{side:?}");
            let back = face_point(side, hit);
            assert!((back[0] - point[0]).abs() < 1e-6 && (back[1] - point[1]).abs() < 1e-6, "{side:?}");
        }
    }

    #[test]
    fn a_hit_and_its_pixel_agree_within_a_pixel_on_every_side_and_rotation() {
        let resolution = [640, 360];
        for side in Side::ALL {
            for rotation in Rotation::ALL {
                for size in [[1, 1], [4, 2], [3, 5]] {
                    let face = Face { side, rotation, size };
                    let drawn = face.drawn();
                    let one_pixel = (drawn[0] / resolution[0] as f32).max(drawn[1] / resolution[1] as f32);
                    for s in [0.01, 0.37, 0.5, 0.99] {
                        for t in [0.01, 0.37, 0.5, 0.99] {
                            let start = hit_at(side, [BEZEL + drawn[0] * s, BEZEL + drawn[1] * t]);
                            let pixel = hit_to_pixel(&face, resolution, start, anywhere).expect("on screen");
                            let back = pixel_to_hit(&face, None, resolution, pixel.map(|p| p as f32));
                            assert!(
                                apart(start, back) <= one_pixel + 1e-4,
                                "{side:?} {rotation:?} {size:?}: {start:?} -> {pixel:?} -> {back:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_client_scales_a_click_to_what_its_browser_renders() {
        let saved = [1920, 1080];
        assert_eq!(to_effective(Rotation::R0, saved, saved, [1900, 1000]), [1900, 1000], "full quality");
        assert_eq!(to_effective(Rotation::R0, saved, [960, 540], [1000, 500]), [500, 250]);
        assert_eq!(to_effective(Rotation::R0, saved, [960, 540], [1001, 501]), [501, 251], "500.5 rounds up");
        // A resolution above the maximum is shrunk on each side by its own ratio.
        assert_eq!(to_effective(Rotation::R0, [4000, 1000], [1920, 480], [2000, 500]), [960, 240]);
        assert_eq!(to_effective(Rotation::R0, [0, 1080], [960, 540], [5, 5]), [5, 5], "nothing to scale by");
    }

    #[test]
    fn a_scaled_click_counts_against_the_swapped_sides_after_a_quarter_turn() {
        // The browser is 1080x1920, rendered at 540x960.
        assert_eq!(to_effective(Rotation::R90, [1920, 1080], [960, 540], [1000, 1900]), [500, 950]);
        assert_eq!(
            to_effective(Rotation::R270, [1920, 1080], [960, 540], [1079, 1919]),
            [539, 959],
            "the last pixel stays the last: 539.5 and 959.5 would round one past"
        );
    }

    #[test]
    fn a_click_on_a_letterboxed_shared_picture_lands_on_the_picture_or_on_nothing() {
        // The drawn area is 3.75 by 1.75 blocks and the shared picture square: it takes the
        // middle 1.75 / 3.75 = 0.4667 of the width, pixels 400 to 1100 of these 1500.
        let face = face(Side::South, Rotation::R0);
        let click = |x, y| to_shared(&face, [1500, 700], [1000, 1000], [x, y]);
        assert_eq!(click(750, 350), Some([500, 500]), "the middle is the middle");
        assert_eq!(click(401, 0), Some([1, 0]), "just inside the left bar");
        assert_eq!(click(1099, 699), Some([999, 999]), "just inside the right one, at the bottom");
        assert_eq!(click(399, 350), None, "on the left bar");
        assert_eq!(click(1101, 350), None, "on the right bar");
        assert_eq!(to_shared(&face, [1500, 700], [0, 1000], [750, 350]), None, "no picture to click");
    }

    #[test]
    fn a_quarter_turn_puts_the_bars_of_a_shared_picture_above_and_below() {
        // The browser frame is now 700 wide and 1500 tall: the square takes rows 400 to 1100.
        let face = face(Side::South, Rotation::R90);
        let click = |x, y| to_shared(&face, [1500, 700], [1000, 1000], [x, y]);
        assert_eq!(click(350, 750), Some([500, 500]));
        assert_eq!(click(350, 100), None, "on the upper bar");
        assert_eq!(click(350, 1400), None, "on the lower bar");
        assert_eq!(click(10, 750), Some([14, 500]), "no bar at the sides: 10 / 700 of 1000");
    }

    #[test]
    fn a_click_on_a_linked_screen_lands_in_its_part_of_the_groups_picture() {
        let face = Face { side: Side::South, rotation: Rotation::R0, size: [2, 2] };
        let own = [1000, 1000];
        // Two screens side by side: the right one shows the right half.
        let right = Tile { offset: [2, 0], group: [4, 2] };
        assert_eq!(to_group(&face, own, &right, [2000, 1000], [250, 100]), [1250, 100]);
        // Two screens one above the other: the upper one shows the upper half.
        let upper = Tile { offset: [0, 2], group: [2, 4] };
        let lower = Tile { offset: [0, 0], group: [2, 4] };
        assert_eq!(to_group(&face, own, &upper, [1000, 2000], [0, 0]), [0, 0], "top left is top left");
        assert_eq!(to_group(&face, own, &upper, [1000, 2000], [500, 500]), [500, 500]);
        assert_eq!(to_group(&face, own, &lower, [1000, 2000], [500, 500]), [500, 1500]);
        assert_eq!(to_group(&face, own, &lower, [1000, 2000], [999, 999]), [999, 1999]);
        // With a gap kept in the picture (SPACED), the far screen starts after it.
        let far = Tile { offset: [5, 0], group: [7, 2] };
        assert_eq!(to_group(&face, own, &far, [7000, 2000], [500, 500]), [6000, 1000]);
    }

    #[test]
    fn a_turned_group_is_turned_as_a_whole() {
        let own = [1000, 1000];
        let left = Tile { offset: [0, 0], group: [4, 2] };
        // Upside down, the screen on the left shows the page's right half.
        let upside_down = Face { side: Side::South, rotation: Rotation::R180, size: [2, 2] };
        assert_eq!(to_group(&upside_down, own, &left, [2000, 1000], [250, 100]), [1250, 100]);
        // A quarter turn: the browser is 1000 wide and 2000 tall, its top at the group's
        // left. The left screen shows the page's upper half.
        let quarter = Face { side: Side::South, rotation: Rotation::R90, size: [2, 2] };
        assert_eq!(to_group(&quarter, own, &left, [2000, 1000], [250, 100]), [250, 100]);
        let right = Tile { offset: [2, 0], group: [4, 2] };
        assert_eq!(to_group(&quarter, own, &right, [2000, 1000], [250, 100]), [250, 1100]);
        // Seen from below, the page's top is south: the screen further north (the side's
        // up) shows the lower half.
        let below = Face { side: Side::Bottom, rotation: Rotation::R0, size: [2, 2] };
        let north = Tile { offset: [0, 2], group: [2, 4] };
        assert_eq!(to_group(&below, own, &north, [1000, 2000], [500, 500]), [500, 1500]);
    }

    #[test]
    fn a_groups_pixel_gives_back_the_point_on_the_screen_that_shows_it() {
        let face = Face { side: Side::South, rotation: Rotation::R0, size: [2, 2] };
        let right = Tile { offset: [2, 0], group: [4, 2] };
        // Three blocks along and one up in the group: the middle of the right screen.
        let hit = pixel_to_hit(&face, Some(&right), [4000, 2000], [3000.0, 1000.0]);
        assert!(apart(hit, [1.0, 1.0, 1.0]) < 1e-5, "{hit:?}");
        // A pixel the left screen shows is off this one, to its left.
        let beside = pixel_to_hit(&face, Some(&right), [4000, 2000], [1000.0, 1000.0]);
        assert!(beside[0] < 0.0, "{beside:?}");
    }

    #[test]
    fn a_hit_on_a_linked_screen_and_its_groups_pixel_agree_on_every_side_and_rotation() {
        let (own, whole) = ([640, 640], [1920, 548]);
        let tile = Tile { offset: [2, 0], group: [7, 2] };
        for side in Side::ALL {
            for rotation in Rotation::ALL {
                let face = Face { side, rotation, size: [2, 2] };
                let drawn = face.drawn();
                // One pixel of the screen, plus one of the group as wide as it is drawn here.
                let one_pixel = drawn[0] / 640.0 + (7.0 / 1920.0_f32).max(2.0 / 548.0) * drawn[0] / 2.0;
                for s in [0.01, 0.37, 0.99] {
                    for t in [0.01, 0.5, 0.99] {
                        let start = hit_at(side, [BEZEL + drawn[0] * s, BEZEL + drawn[1] * t]);
                        let pixel = hit_to_pixel(&face, own, start, anywhere).expect("on the screen");
                        let shared = to_group(&face, own, &tile, whole, pixel);
                        let back = pixel_to_hit(&face, Some(&tile), whole, shared.map(|p| p as f32));
                        assert!(
                            apart(start, back) <= one_pixel + 1e-4,
                            "{side:?} {rotation:?}: {start:?} -> {pixel:?} -> {shared:?} -> {back:?}"
                        );
                    }
                }
            }
        }
    }
}
