//! Rays through the blocks: what a sound's path meets.
//!
//! Blocks are whole cubes here. A ray walks the grid one cube at a time (Amanatides and
//! Woo), so it misses none and looks at each once. What a cube is to sound is asked of a
//! `World`, which the game answers from a snapshot of its chunks and the tests from a
//! handful of blocks.
use super::materials::Material;

pub type Vec3 = [f64; 3];
pub type Pos = [i32; 3];

/// The blocks, as far as sound needs them.
pub trait World {
    /// What the block at `pos` does to sound; None where sound passes freely.
    fn at(&self, pos: Pos) -> Option<Material>;
}

impl<F: Fn(Pos) -> Option<Material>> World for F {
    fn at(&self, pos: Pos) -> Option<Material> {
        self(pos)
    }
}

pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale(a: Vec3, by: f64) -> Vec3 {
    [a[0] * by, a[1] * by, a[2] * by]
}

pub fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn length(a: Vec3) -> f64 {
    dot(a, a).sqrt()
}

/// `a` at length 1; zero stays zero.
pub fn unit(a: Vec3) -> Vec3 {
    let long = length(a);
    if long < 1e-12 { [0.0; 3] } else { scale(a, 1.0 / long) }
}

/// `direction` bounced off a surface facing `normal`.
pub fn reflect(direction: Vec3, normal: Vec3) -> Vec3 {
    sub(direction, scale(normal, 2.0 * dot(direction, normal)))
}

/// The block a point is in.
pub fn block_of(point: Vec3) -> Pos {
    point.map(|part| part.floor() as i32)
}

/// Where a ray met a wall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub block: Pos,
    pub point: Vec3,
    /// The face it came in by, pointing back out of the block.
    pub normal: Vec3,
    /// From the ray's start.
    pub distance: f64,
    pub material: Material,
}

/// Walks the cubes from `from` along `direction` (a unit vector) for `reach` blocks, the
/// one `from` is in left out. `visit` is given each cube, the distance at which the ray
/// enters it and the face it enters by; it returns true to stop there.
fn walk(from: Vec3, direction: Vec3, reach: f64, mut visit: impl FnMut(Pos, f64, Vec3) -> bool) {
    let mut block = block_of(from);
    let step = direction.map(|part| if part > 0.0 { 1 } else { -1 });
    // The distance along the ray between two grid planes of an axis, and to the first one.
    let between = direction.map(|part| if part.abs() < 1e-12 { f64::INFINITY } else { 1.0 / part.abs() });
    let mut next = [0, 1, 2].map(|i| {
        if between[i].is_infinite() {
            return f64::INFINITY;
        }
        let edge = if direction[i] > 0.0 { f64::from(block[i]) + 1.0 - from[i] } else { from[i] - f64::from(block[i]) };
        edge * between[i]
    });
    loop {
        let axis = if next[0] <= next[1] && next[0] <= next[2] {
            0
        } else if next[1] <= next[2] {
            1
        } else {
            2
        };
        let entered = next[axis];
        if entered > reach {
            return;
        }
        block[axis] += step[axis];
        next[axis] += between[axis];
        let mut normal = [0.0; 3];
        normal[axis] = -f64::from(step[axis]);
        if visit(block, entered, normal) {
            return;
        }
    }
}

/// The first wall along the ray from `from` in `direction` (a unit vector) within `reach`
/// blocks. The block `from` is in does not count: a ray leaving a surface starts in it.
pub fn cast(world: &impl World, from: Vec3, direction: Vec3, reach: f64) -> Option<Hit> {
    let mut hit = None;
    walk(from, direction, reach, |block, distance, normal| {
        match world.at(block) {
            Some(material) if material.wall => {
                hit = Some(Hit { block, point: add(from, scale(direction, distance)), normal, distance, material });
                true
            }
            _ => false,
        }
    });
    hit
}

/// How much the blocks on the straight line from `from` to `to` muffle: the sum of their
/// occlusion, the blocks the two ends are in left out (a speaker is in its own block, the
/// listener's head in none). Stops counting past `enough`: more is silence anyway.
pub fn occlusion(world: &impl World, from: Vec3, to: Vec3, enough: f32) -> f32 {
    let path = sub(to, from);
    let (far, last) = (length(path), block_of(to));
    let mut sum = 0.0;
    walk(from, unit(path), far, |block, _, _| {
        if block != last
            && let Some(material) = world.at(block)
        {
            sum += material.occlusion;
        }
        sum >= enough
    });
    sum
}

/// Whether nothing stands between `from` and `to`: no wall on the line, the two ends'
/// own blocks left out.
pub fn clear(world: &impl World, from: Vec3, to: Vec3) -> bool {
    let path = sub(to, from);
    let (far, last) = (length(path), block_of(to));
    let mut free = true;
    walk(from, unit(path), far, |block, _, _| {
        if block != last && world.at(block).is_some_and(|material| material.wall) {
            free = false;
        }
        !free
    });
    free
}

#[cfg(test)]
mod tests {
    use super::*;

    const STONE: Material = Material { reflectivity: 1.5, occlusion: 1.0, wall: true };
    const FENCE: Material = Material { reflectivity: 0.4, occlusion: 0.25, wall: false };

    /// A wall of stone filling x = 5, with a fence at x = 3.
    fn wall(pos: Pos) -> Option<Material> {
        match pos[0] {
            5 => Some(STONE),
            3 => Some(FENCE),
            _ => None,
        }
    }

    #[test]
    fn a_ray_stops_at_the_first_wall_and_says_which_face() {
        let hit = cast(&wall, [0.5, 0.5, 0.5], [1.0, 0.0, 0.0], 32.0).unwrap();
        assert_eq!(hit.block, [5, 0, 0]);
        assert!((hit.distance - 4.5).abs() < 1e-9 && (hit.point[0] - 5.0).abs() < 1e-9);
        assert_eq!(hit.normal, [-1.0, 0.0, 0.0], "it faces back at the ray");
        assert_eq!(hit.material, STONE);
        let back = cast(&wall, [9.5, 0.5, 0.5], [-1.0, 0.0, 0.0], 32.0).unwrap();
        assert_eq!((back.block, back.normal), ([5, 0, 0], [1.0, 0.0, 0.0]));
    }

    #[test]
    fn a_ray_passes_what_is_no_wall_and_gives_up_past_its_reach() {
        assert_eq!(cast(&wall, [0.5, 0.5, 0.5], [1.0, 0.0, 0.0], 4.0), None, "the wall is 4.5 away");
        assert_eq!(cast(&wall, [0.5, 0.5, 0.5], [0.0, 1.0, 0.0], 64.0), None, "along the wall");
        assert_eq!(cast(&wall, [0.5, 0.5, 0.5], [-1.0, 0.0, 0.0], 64.0), None, "away from it");
    }

    #[test]
    fn a_slanted_ray_misses_no_cube() {
        // A single block far off the axes, aimed at through its middle.
        let target = [7, 3, -4];
        let world = |pos: Pos| (pos == target).then_some(STONE);
        let from = [0.2, 0.7, 0.4];
        let direction = unit(sub([7.5, 3.5, -3.5], from));
        let hit = cast(&world, from, direction, 64.0).unwrap();
        assert_eq!(hit.block, target);
        // The point is on the block's surface.
        let on_surface = (0..3).any(|i| (hit.point[i] - f64::from(target[i])).abs() < 1e-9 || (hit.point[i] - f64::from(target[i]) - 1.0).abs() < 1e-9);
        assert!(on_surface, "{:?}", hit.point);
    }

    #[test]
    fn a_ray_leaving_a_surface_does_not_hit_the_block_it_starts_in() {
        let hit = cast(&wall, [5.0, 0.5, 0.5], [-1.0, 0.0, 0.0], 64.0);
        assert_eq!(hit, None, "it starts on the wall's face and goes away from it");
    }

    #[test]
    fn what_is_in_the_way_muffles_by_what_it_is() {
        assert_eq!(occlusion(&wall, [0.5, 0.5, 0.5], [9.5, 0.5, 0.5], 10.0), 1.25, "a fence and a wall");
        assert_eq!(occlusion(&wall, [0.5, 0.5, 0.5], [2.5, 0.5, 0.5], 10.0), 0.0, "nothing between");
        assert_eq!(occlusion(&wall, [0.5, 0.5, 0.5], [5.5, 0.5, 0.5], 10.0), 0.25, "the listener's own block does not count");
        let thick = |pos: Pos| (2..=8).contains(&pos[0]).then_some(STONE);
        assert_eq!(occlusion(&thick, [0.5, 0.5, 0.5], [20.5, 0.5, 0.5], 3.0), 3.0, "counted no further than enough");
    }

    #[test]
    fn a_line_is_clear_when_no_wall_is_on_it() {
        assert!(clear(&wall, [0.5, 0.5, 0.5], [4.5, 0.5, 0.5]), "a fence is no wall");
        assert!(!clear(&wall, [0.5, 0.5, 0.5], [9.5, 0.5, 0.5]));
        assert!(clear(&wall, [0.5, 0.5, 0.5], [0.5, 30.5, 0.5]));
    }

    #[test]
    fn a_bounce_mirrors_the_ray() {
        assert_eq!(reflect([1.0, -1.0, 0.0], [0.0, 1.0, 0.0]), [1.0, 1.0, 0.0]);
        assert_eq!(reflect([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]), [-1.0, 0.0, 0.0]);
    }
}
