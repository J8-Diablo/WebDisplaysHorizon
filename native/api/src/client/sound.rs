//! Where a screen's sound comes from, how loud it is at a distance, and which way that is
//! from the listener's head: the geometry of the scene hcef-host plays (`proto::audio`).
//!
//! A screen is a stereo surface: the page's left channel plays from its left half, the right
//! one from its right half, each from the point of that half nearest the listener. Walking
//! along a wide screen, the sound stays beside you instead of jumping to the middle.
//!
//! Plain arithmetic, tested without the game.
use crate::model::hit::{block_point, face_point, face_to_picture};
use crate::model::side::{Pos, Rotation, Side};

/// How far in front of the blocks a screen's sound starts, in blocks: outside the wall, so
/// that rays cast from it start in the air.
const OUT: f64 = 0.35;
/// How far inside a half's edges its sound stays, in blocks.
const MARGIN: f32 = 0.25;
/// The distance up to which a screen plays at its full volume, in blocks.
pub const NEAR: f64 = 5.0;
/// The distance at which a screen with automatic volume is silent, in blocks.
pub const RANGE: f64 = 48.0;

/// The two points a screen's sound comes from, for a listener at `listener`: the left
/// channel's, then the right one's, as the picture is seen (its rotation included).
pub fn screen_points(origin: Pos, side: Side, rotation: Rotation, size: [u32; 2], listener: [f64; 3]) -> [[f64; 3]; 2] {
    let [width, height] = size.map(|blocks| blocks.max(1) as f32);
    // Which way the picture's left-to-right runs on the face: along the side's right or its
    // up, and in which sense. Asked of the same function that draws it.
    let across = |point: [f32; 2]| face_to_picture(side, rotation, point)[0];
    let along_right = across([1.0, 0.5]) - across([0.0, 0.5]);
    let along_up = across([0.5, 1.0]) - across([0.5, 0.0]);
    let (axis, forwards) = if along_right.abs() >= along_up.abs() { (0, along_right > 0.0) } else { (1, along_up > 0.0) };

    // The listener on the face, in blocks along its right and up from the lower-left corner.
    let local = [0, 1, 2].map(|i| (listener[i] - f64::from(origin[i])) as f32);
    let over = face_point(side, local);
    let extent = [width, height];
    let forward = side.forward();
    let point = |low: f32, high: f32| -> [f64; 3] {
        let mut at = [0f32; 2];
        for i in 0..2 {
            let (from, to) = if i == axis { (low, high) } else { (0.0, extent[i]) };
            // A half narrower than two margins plays from its middle.
            let (from, to) = if to - from > 2.0 * MARGIN { (from + MARGIN, to - MARGIN) } else { ((from + to) / 2.0, (from + to) / 2.0) };
            at[i] = over[i].clamp(from, to);
        }
        let on_block = block_point(side, at);
        [0, 1, 2].map(|i| f64::from(origin[i]) + f64::from(on_block[i]) + f64::from(forward[i]) * OUT)
    };
    let half = extent[axis] / 2.0;
    let (first, second) = (point(0.0, half), point(half, extent[axis]));
    if forwards { [first, second] } else { [second, first] }
}

/// How loud a sound `distance` blocks away is, 0 to 1. With `fades` (a screen's automatic
/// volume) it falls with distance, as in air, and reaches nothing at `RANGE`; without, it
/// is as loud everywhere it is heard at all.
pub fn loudness(distance: f64, fades: bool) -> f32 {
    if !fades || distance <= NEAR {
        return 1.0;
    }
    if distance >= RANGE {
        return 0.0;
    }
    // Half as loud each time the distance doubles, then eased to silence over the last
    // quarter of the range so that nothing cuts off.
    let inverse = NEAR / distance;
    let last = ((RANGE - distance) / (RANGE * 0.25)).clamp(0.0, 1.0);
    (inverse * last * last * (3.0 - 2.0 * last)) as f32
}

/// Which way `point` is from a listener at `eye` looking through `view` (the camera's view
/// matrix, column-major, as the game gives it): a unit vector, x to the right, y up, z
/// ahead. Zero for a point at the listener's own place.
pub fn direction(eye: [f64; 3], view: &[f32; 16], point: [f64; 3]) -> [f32; 3] {
    let offset = [0, 1, 2].map(|i| (point[i] - eye[i]) as f32);
    let long = offset.iter().map(|part| part * part).sum::<f32>().sqrt();
    if long < 1e-4 {
        return [0.0; 3];
    }
    let row = |r: usize| (view[r] * offset[0] + view[4 + r] * offset[1] + view[8 + r] * offset[2]) / long;
    // A camera looks down its own -z.
    [row(0), row(1), -row(2)]
}

/// Where a speaker block's sound starts: just outside the face it plays from (`facing`, a
/// direction's name), in the air. A speaker that says no direction plays from its middle.
pub fn speaker_point(pos: Pos, facing: Option<&str>) -> ([f64; 3], [f64; 3]) {
    let way: [f64; 3] = match facing {
        Some("down") => [0.0, -1.0, 0.0],
        Some("up") => [0.0, 1.0, 0.0],
        Some("north") => [0.0, 0.0, -1.0],
        Some("south") => [0.0, 0.0, 1.0],
        Some("west") => [-1.0, 0.0, 0.0],
        Some("east") => [1.0, 0.0, 0.0],
        _ => [0.0; 3],
    };
    let point = [0, 1, 2].map(|i| f64::from(pos[i]) + 0.5 + way[i] * (0.5 + OUT));
    (point, way)
}

/// How much of a speaker's sound goes toward a listener, 0 to 1: `focus` 0 plays the same
/// all round (a bass speaker), 1 plays ahead and next to nothing behind (a tweeter).
/// `facing` is the way the speaker plays (zero for none), `toward` the way to the listener.
pub fn beam(facing: [f64; 3], toward: [f64; 3], focus: f32) -> f32 {
    let long = toward.iter().map(|part| part * part).sum::<f64>().sqrt();
    if long < 1e-6 || facing == [0.0; 3] {
        return 1.0;
    }
    let ahead = ((0..3).map(|i| facing[i] * toward[i]).sum::<f64>() / long) as f32;
    // From 1 straight ahead down to 1 - focus straight behind.
    1.0 - focus.clamp(0.0, 1.0) * (1.0 - ahead) / 2.0
}

pub fn distance(from: [f64; 3], to: [f64; 3]) -> f64 {
    (0..3).map(|i| (to[i] - from[i]).powi(2)).sum::<f64>().sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A camera looking north (-z), upright: the identity.
    const NORTH: [f32; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

    #[test]
    fn a_screen_plays_its_left_channel_on_the_viewers_left() {
        // A 4x2 screen on the south face of blocks at z = 0, seen from the south looking north:
        // the viewer's left is west (smaller x).
        let listener = [2.0, 65.0, 6.0];
        let [left, right] = screen_points([0, 64, 0], Side::South, Rotation::R0, [4, 2], listener);
        assert!(left[0] < right[0], "{left:?} {right:?}");
        assert!(left[0] < 2.0 && right[0] > 2.0, "each in its half");
        assert!((left[2] - (1.0 + OUT)).abs() < 1e-6, "in front of the face, in the air");
        // The same screen seen from the other side of the world: on the north face the
        // viewer's left is east.
        let [left, right] = screen_points([3, 64, 0], Side::North, Rotation::R0, [4, 2], [2.0, 65.0, -6.0]);
        assert!(left[0] > right[0], "{left:?} {right:?}");
        assert!((left[2] + OUT).abs() < 1e-6);
    }

    #[test]
    fn the_sound_stays_beside_a_listener_walking_along_the_screen() {
        // A 16-wide screen; the listener stands in front of its far left.
        let [left, right] = screen_points([0, 64, 0], Side::South, Rotation::R0, [16, 4], [1.0, 65.5, 3.0]);
        assert!((left[0] - 1.0).abs() < 1e-6 && (left[1] - 65.5).abs() < 1e-6, "right in front of them: {left:?}");
        assert!((right[0] - (8.0 + f64::from(MARGIN))).abs() < 1e-6, "the nearest point of the right half: {right:?}");
        // Beyond the screen's end, the sound stops at its edge.
        let [left, _] = screen_points([0, 64, 0], Side::South, Rotation::R0, [16, 4], [-9.0, 80.0, 3.0]);
        assert!((left[0] - f64::from(MARGIN)).abs() < 1e-6 && (left[1] - (68.0 - f64::from(MARGIN))).abs() < 1e-6, "{left:?}");
    }

    #[test]
    fn a_turned_picture_turns_its_channels_with_it() {
        let listener = [2.0, 66.0, 6.0];
        let upright = screen_points([0, 64, 0], Side::South, Rotation::R0, [4, 4], listener);
        let upside_down = screen_points([0, 64, 0], Side::South, Rotation::R180, [4, 4], listener);
        assert!(upside_down[0][0] > upside_down[1][0], "left and right swap: {upside_down:?}");
        assert_eq!(upright[0], upside_down[1]);
        let quarter = screen_points([0, 64, 0], Side::South, Rotation::R90, [4, 4], listener);
        assert!((quarter[0][0] - quarter[1][0]).abs() < 1e-6, "the channels are one above the other: {quarter:?}");
        assert!((quarter[0][1] - quarter[1][1]).abs() > 0.4);
    }

    #[test]
    fn a_screen_one_block_wide_plays_from_its_middle() {
        let [left, right] = screen_points([0, 64, 0], Side::South, Rotation::R0, [1, 2], [9.0, 64.0, 3.0]);
        assert!((left[0] - 0.25).abs() < 1e-6 && (right[0] - 0.75).abs() < 1e-6, "{left:?} {right:?}");
    }

    #[test]
    fn a_sound_fades_with_distance_only_when_asked_to() {
        assert_eq!(loudness(0.0, true), 1.0);
        assert_eq!(loudness(NEAR, true), 1.0);
        assert!((loudness(2.0 * NEAR, true) - 0.5).abs() < 1e-6, "half as loud at twice the distance");
        assert_eq!(loudness(RANGE, true), 0.0);
        assert_eq!(loudness(RANGE * 3.0, true), 0.0);
        let mut last = 1.0;
        for step in 0..200 {
            let now = loudness(f64::from(step) * 0.3, true);
            assert!(now <= last, "never louder further away (at {})", f64::from(step) * 0.3);
            last = now;
        }
        assert_eq!(loudness(RANGE * 3.0, false), 1.0, "a fixed volume is the same anywhere");
    }

    #[test]
    fn a_speaker_plays_from_in_front_of_its_face() {
        let (point, way) = speaker_point([10, 64, -3], Some("east"));
        assert_eq!(way, [1.0, 0.0, 0.0]);
        assert!((point[0] - (11.0 + OUT)).abs() < 1e-9 && point[1] == 64.5 && point[2] == -2.5, "{point:?}");
        let (point, way) = speaker_point([10, 64, -3], None);
        assert_eq!((point, way), ([10.5, 64.5, -2.5], [0.0; 3]));
    }

    #[test]
    fn a_tweeter_beams_and_a_bass_speaker_does_not() {
        let east = [1.0, 0.0, 0.0];
        assert_eq!(beam(east, [5.0, 0.0, 0.0], 1.0), 1.0, "straight ahead");
        assert_eq!(beam(east, [-5.0, 0.0, 0.0], 1.0), 0.0, "straight behind a tweeter");
        assert_eq!(beam(east, [0.0, 0.0, 3.0], 1.0), 0.5, "beside it");
        assert_eq!(beam(east, [-5.0, 0.0, 0.0], 0.0), 1.0, "a bass speaker plays all round");
        assert_eq!(beam(east, [-5.0, 0.0, 0.0], 0.4), 0.6);
        assert_eq!(beam([0.0; 3], [1.0, 0.0, 0.0], 1.0), 1.0, "no direction, no beam");
        assert_eq!(beam(east, [0.0; 3], 1.0), 1.0, "the listener is at the speaker");
    }

    #[test]
    fn a_direction_is_told_from_the_listeners_head() {
        let eye = [0.0, 64.0, 0.0];
        assert_eq!(direction(eye, &NORTH, [0.0, 64.0, -5.0]), [0.0, 0.0, 1.0], "ahead");
        assert_eq!(direction(eye, &NORTH, [3.0, 64.0, 0.0]), [1.0, 0.0, 0.0], "to the right (east, looking north)");
        assert_eq!(direction(eye, &NORTH, [0.0, 70.0, 0.0]), [0.0, 1.0, 0.0], "above");
        assert_eq!(direction(eye, &NORTH, [0.0, 64.0, 2.0]), [0.0, 0.0, -1.0], "behind");
        assert_eq!(direction(eye, &NORTH, eye), [0.0; 3]);
        // Looking east (+x): the view turns the world so that +x lands on -z.
        let east: [f32; 16] = [0.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        assert_eq!(direction(eye, &east, [4.0, 64.0, 0.0]), [0.0, 0.0, 1.0], "ahead");
        assert_eq!(direction(eye, &east, [0.0, 64.0, 4.0]), [1.0, 0.0, 0.0], "south is to the right");
    }
}
