//! What a place does to a sound on its way to the listener: how much the blocks between
//! muffle it, and how much of it comes back from the walls around, for how long.
//!
//! Rays leave the sound in every direction and bounce a few times, as in Sound Physics
//! Remastered, whose block tables and whose handling of the straight path are kept: the
//! blocks on the line muffle, and a bounce that can see the listener means the two share an
//! air space, so the sound still arrives round the corner.
//!
//! How long the place rings is worked out differently. Sound Physics adds up the length of
//! a ray's whole path, which makes any stone room, a closet or a cathedral, ring at full
//! length. Here the rays measure the room instead: the mean distance between two walls and
//! how much each wall takes give a decay time by Eyring's formula (the one architects
//! use), and rays that escape count as walls that take everything. A small stone room
//! rings short and bright, a hall long, a wool-lined room hardly, the open air not at all.
//! The energy then goes to the two of hcef-host's four reverberations nearest that time.
//!
//! Pure arithmetic on a `World`: tested on rooms made of a few lines of code.
use super::trace::{Vec3, World, add, cast, clear, occlusion, reflect, scale};

/// Rays cast from a sound.
pub const RAYS: usize = 32;
/// Bounces followed after a ray's first hit.
pub const BOUNCES: usize = 4;
/// How far a ray goes before it is taken to have left for the sky, in blocks.
pub const REACH: f64 = 96.0;
/// How long each of hcef-host's reverberations lasts, in seconds (`proto::audio`).
pub const DECAYS: [f32; 4] = [0.15, 0.55, 1.68, 4.142];
/// Occlusion past which the straight path carries nothing worth working out.
const DEAF: f32 = 10.0;
/// What a ray that escapes counts as: a wall that sends back one part in a hundred.
const OPEN_LOSS: f32 = 4.6;

/// What a place does to one sound for one listener.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Heard {
    /// The level of the straight path: 1 with nothing in the way.
    pub gain: f32,
    /// How much of its treble the straight path keeps: 1 with nothing in the way.
    pub treble: f32,
    /// How much of the sound each reverberation gets, shortest first, 0 to 1.
    pub sends: [f32; 4],
}

impl Heard {
    /// As in the open with nothing around.
    pub const DRY: Heard = Heard { gain: 1.0, treble: 1.0, sends: [0.0; 4] };

    /// Part of the way from this to `other`: `by` 0 stays, 1 arrives. For gliding between
    /// two answers instead of jumping.
    pub fn toward(&self, other: &Heard, by: f32) -> Heard {
        let mix = |a: f32, b: f32| a + (b - a) * by.clamp(0.0, 1.0);
        Heard {
            gain: mix(self.gain, other.gain),
            treble: mix(self.treble, other.treble),
            sends: [0, 1, 2, 3].map(|i| mix(self.sends[i], other.sends[i])),
        }
    }
}

/// The directions of the rays: points spread evenly over a sphere (a golden spiral).
fn directions() -> impl Iterator<Item = Vec3> {
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    (0..RAYS).map(move |i| {
        let latitude = ((i as f64 + 0.5) / RAYS as f64 * 2.0 - 1.0).asin();
        let longitude = golden * i as f64;
        [latitude.cos() * longitude.cos(), latitude.sin(), latitude.cos() * longitude.sin()]
    })
}

/// The share of a sound's energy a wall sends back, from its Sound Physics reflectivity
/// (0.1 for wool to 1.5 for stone): bare stone keeps nearly all of it, wool next to none.
fn sent_back(reflectivity: f32) -> f32 {
    (reflectivity * 0.62 + 0.02).clamp(0.02, 0.95)
}

/// What the rays found out about a place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Room {
    /// How closed it is: the share of the rays' flights that ended on a wall. 1 in a
    /// sealed room, about a third on open ground.
    pub enclosure: f32,
    /// How long it rings, in seconds (the time a sound takes to fall by 60 dB).
    pub decay: f32,
    /// The mean share of energy its walls send back.
    pub liveliness: f32,
    /// The share of the bounces from which the listener can be seen.
    pub shared: f32,
}

/// Measures the place around `source`, as heard from `listener`.
pub fn measure(world: &impl World, source: Vec3, listener: Vec3) -> Room {
    let (mut hits, mut escapes, mut seen) = (0u32, 0u32, 0u32);
    // Summed over the hits: what each wall takes (as Eyring counts it) and sends back.
    let (mut loss, mut back) = (0f32, 0f32);
    // The flights from one wall to the next, and from the sound to its first wall.
    let (mut between, mut flights, mut first, mut firsts) = (0f64, 0u32, 0f64, 0u32);
    for direction in directions() {
        let Some(mut last) = cast(world, source, direction, REACH) else {
            escapes += 1;
            continue;
        };
        first += last.distance;
        firsts += 1;
        let mut heading = direction;
        for _ in 0..=BOUNCES {
            hits += 1;
            let kept = sent_back(last.material.reflectivity);
            loss += -kept.ln();
            back += kept;
            if clear(world, add(last.point, scale(last.normal, 0.001)), listener) {
                seen += 1;
            }
            heading = reflect(heading, last.normal);
            match cast(world, last.point, heading, REACH) {
                Some(next) => {
                    between += next.distance;
                    flights += 1;
                    last = next;
                }
                None => {
                    escapes += 1;
                    break;
                }
            }
        }
    }
    if hits == 0 {
        return Room { enclosure: 0.0, decay: 0.0, liveliness: 0.0, shared: 0.0 };
    }
    let events = (hits + escapes) as f32;
    // Without a wall-to-wall flight (open ground) the way to the first wall stands in.
    let free_path = if flights > 0 { between / f64::from(flights) } else { first / f64::from(firsts.max(1)) };
    let mean_loss = (loss + escapes as f32 * OPEN_LOSS) / events;
    // Eyring: 0.161 V / (S x loss), with the mean free path 4V/S: 0.04 x path / loss.
    let decay = 0.04 * free_path as f32 / mean_loss.max(0.01);
    Room { enclosure: hits as f32 / events, decay, liveliness: back / hits as f32, shared: seen as f32 / hits as f32 }
}

/// How a reverberation of `decay` seconds, `level` strong, is shared between the four
/// hcef-host plays: the two that last nearest as long, by how near (on a scale of ratios).
/// Shorter than the shortest, it fades out with it.
fn spread(decay: f32, level: f32) -> [f32; 4] {
    let mut sends = [0f32; 4];
    if decay <= DECAYS[0] {
        sends[0] = level * (decay / DECAYS[0]).max(0.0);
        return sends;
    }
    let last = DECAYS.len() - 1;
    if decay >= DECAYS[last] {
        sends[last] = level;
        return sends;
    }
    let above = DECAYS.iter().position(|step| decay < *step).unwrap_or(last);
    let part = (decay / DECAYS[above - 1]).ln() / (DECAYS[above] / DECAYS[above - 1]).ln();
    sends[above - 1] = level * (1.0 - part);
    sends[above] = level * part;
    sends
}

/// What `world` does to a sound at `source` for a listener at `listener`.
pub fn evaluate(world: &impl World, source: Vec3, listener: Vec3) -> Heard {
    let blocked = occlusion(world, source, listener, DEAF);
    let room = measure(world, source, listener);

    // How much the listener is in the same air as the sound: full from a fifth of the
    // bounces seeing them (Sound Physics' weights, the short reverberation needing most).
    let weights = [0.31, 0.23, 0.16, 0.16].map(|full: f32| (room.shared / full).clamp(0.0, 1.0));
    let together = weights.iter().sum::<f32>() * 0.25;
    // A sound round a corner still arrives, darker: the straight path keeps a little of it.
    let treble = (-blocked).exp().max(together.sqrt() * 0.2);
    let gain = treble.powf(0.1);

    // A place answers by how closed it is and how hard its walls are.
    let level = room.enclosure * room.enclosure * room.liveliness;
    let mut sends = spread(room.decay, level);
    // What is walled off reaches the listener's ears less, unless the air is shared.
    let absorbed = [1.0, 1.0, 1.5, 1.5];
    for i in 0..4 {
        let open = (-blocked * absorbed[i]).exp() * (1.0 - weights[i]) + weights[i];
        sends[i] = sends[i].clamp(0.0, 1.0) * open.powf(0.1);
    }
    Heard { gain, treble, sends }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acoustics::materials::Material;
    use crate::acoustics::trace::{Pos, length};

    const STONE: Material = Material { reflectivity: 1.5, occlusion: 1.0, wall: true };
    const WOOD: Material = Material { reflectivity: 0.4, occlusion: 1.0, wall: true };
    const WOOL: Material = Material { reflectivity: 0.1, occlusion: 1.5, wall: true };
    const GRASS: Material = Material { reflectivity: 0.3, occlusion: 1.0, wall: true };

    /// A flat world: ground below y = 0, nothing above.
    fn field(pos: Pos) -> Option<Material> {
        (pos[1] < 0).then_some(GRASS)
    }

    /// A closed box of `material` whose inside runs from 0 to `size` (exclusive) each way.
    fn room(size: [i32; 3], material: Material) -> impl Fn(Pos) -> Option<Material> {
        move |pos| {
            let inside = (0..3).all(|i| (0..size[i]).contains(&pos[i]));
            let shell = (0..3).all(|i| (-1..=size[i]).contains(&pos[i]));
            (shell && !inside).then_some(material)
        }
    }

    fn total(heard: &Heard) -> f32 {
        heard.sends.iter().sum()
    }

    #[test]
    fn the_rays_go_every_way() {
        let all: Vec<Vec3> = directions().collect();
        assert_eq!(all.len(), RAYS);
        for direction in &all {
            assert!((length(*direction) - 1.0).abs() < 1e-9);
        }
        let sum = all.iter().fold([0.0; 3], |sum, direction| add(sum, *direction));
        assert!(length(sum) < 1.0, "they cancel out: none of the sphere is favoured ({sum:?})");
        assert!(all.iter().any(|d| d[1] > 0.9) && all.iter().any(|d| d[1] < -0.9), "up and down are covered");
    }

    #[test]
    fn in_the_open_a_sound_is_dry_and_whole() {
        let heard = evaluate(&field, [0.5, 1.5, 0.5], [6.5, 1.6, 0.5]);
        assert_eq!((heard.gain, heard.treble), (1.0, 1.0));
        assert!(total(&heard) < 0.05, "only the ground answers: {:?}", heard.sends);
        let nothing = evaluate(&|_: Pos| None, [0.5, 1.5, 0.5], [6.5, 1.6, 0.5]);
        assert_eq!(nothing, Heard::DRY, "in the void");
    }

    #[test]
    fn a_room_rings_by_its_size_and_its_walls() {
        let at = |size: [i32; 3], material: Material| {
            let middle = size.map(|side| f64::from(side) / 2.0);
            measure(&room(size, material), middle, add(middle, [1.0, 0.0, 1.0]))
        };
        let (closet, chamber, hall) = (at([3, 3, 3], STONE), at([7, 4, 7], STONE), at([30, 12, 30], STONE));
        for place in [closet, chamber, hall] {
            assert_eq!((place.enclosure, place.shared), (1.0, 1.0), "sealed, and the listener is inside");
        }
        assert!(closet.decay < chamber.decay && chamber.decay < hall.decay, "{closet:?} {chamber:?} {hall:?}");
        assert!((1.0..5.0).contains(&chamber.decay), "a bare stone room rings a few seconds: {}", chamber.decay);
        assert!(hall.decay > 5.0, "a stone hall rings on: {}", hall.decay);
        let (wooden, padded) = (at([7, 4, 7], WOOD), at([7, 4, 7], WOOL));
        assert!(wooden.decay < 0.4, "a wooden room is short: {}", wooden.decay);
        assert!(padded.decay < 0.1 && padded.liveliness < 0.1, "a padded one is dead: {padded:?}");
    }

    #[test]
    fn the_energy_goes_to_the_reverberations_nearest_in_length() {
        assert_eq!(spread(0.15, 1.0), [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(spread(4.142, 0.5), [0.0, 0.0, 0.0, 0.5]);
        assert_eq!(spread(60.0, 1.0), [0.0, 0.0, 0.0, 1.0], "nothing lasts longer than the longest");
        let between = spread(1.0, 1.0);
        assert!(between[0] == 0.0 && between[3] == 0.0 && between[1] > 0.0 && between[2] > 0.0, "{between:?}");
        assert!((between[1] + between[2] - 1.0).abs() < 1e-6, "all of it, no more");
        assert!(between[2] > between[1], "one second is nearer 1.68 than 0.55, as ratios go");
        assert_eq!(spread(0.075, 1.0), [0.5, 0.0, 0.0, 0.0], "too short to be an echo: it fades");
        assert_eq!(spread(0.0, 1.0), [0.0; 4]);
    }

    #[test]
    fn a_stone_room_rings_and_a_bigger_one_rings_longer() {
        let small = evaluate(&room([5, 4, 5], STONE), [1.5, 1.5, 1.5], [3.5, 1.6, 3.5]);
        let hall = evaluate(&room([30, 12, 30], STONE), [5.5, 1.5, 5.5], [20.5, 1.6, 20.5]);
        assert_eq!((small.gain, small.treble), (1.0, 1.0), "nothing between the two");
        assert!(total(&small) > 0.5 && total(&hall) > 0.5, "{:?} {:?}", small.sends, hall.sends);
        assert!(hall.sends[3] > small.sends[3], "the hall's echo is the long one: {:?} against {:?}", hall.sends, small.sends);
        assert!(small.sends[2] > hall.sends[2], "the small room's is shorter");
    }

    #[test]
    fn a_room_lined_with_wool_hardly_answers() {
        let stone = evaluate(&room([7, 4, 7], STONE), [1.5, 1.5, 1.5], [5.5, 1.6, 5.5]);
        let wool = evaluate(&room([7, 4, 7], WOOL), [1.5, 1.5, 1.5], [5.5, 1.6, 5.5]);
        assert!(total(&wool) < total(&stone) * 0.1, "{:?} against {:?}", wool.sends, stone.sends);
    }

    #[test]
    fn a_roof_overhead_is_heard() {
        // A stone floor, and the same with a stone ceiling four blocks up: a covered market.
        let floor = |pos: Pos| (pos[1] < 0).then_some(STONE);
        let covered = |pos: Pos| (pos[1] < 0 || pos[1] >= 4).then_some(STONE);
        let open = evaluate(&floor, [0.5, 1.5, 0.5], [4.5, 1.6, 0.5]);
        let under = evaluate(&covered, [0.5, 1.5, 0.5], [4.5, 1.6, 0.5]);
        assert!(total(&under) > total(&open) * 3.0, "{:?} against {:?}", under.sends, open.sends);
    }

    #[test]
    fn a_wall_between_muffles_and_a_thicker_one_more() {
        // Two rooms side by side along x, a wall of the given thickness between them.
        let split = |thick: i32| {
            move |pos: Pos| {
                let shell = (-1..=20 + thick).contains(&pos[0]) && (-1..=4).contains(&pos[1]) && (-1..=6).contains(&pos[2]);
                let left = (0..10).contains(&pos[0]) && (0..4).contains(&pos[1]) && (0..6).contains(&pos[2]);
                let right = (10 + thick..20 + thick).contains(&pos[0]) && (0..4).contains(&pos[1]) && (0..6).contains(&pos[2]);
                (shell && !left && !right).then_some(STONE)
            }
        };
        let one = evaluate(&split(1), [5.5, 1.5, 3.5], [14.5, 1.6, 3.5]);
        let three = evaluate(&split(3), [5.5, 1.5, 3.5], [16.5, 1.6, 3.5]);
        assert!(one.treble < 0.5 && one.gain < 1.0, "{one:?}");
        assert!(three.treble < one.treble * 0.5, "{three:?} against {one:?}");
        assert!(three.gain < one.gain);
        assert!(three.gain > 0.5, "muffled, not silenced: the bass comes through");
    }

    #[test]
    fn round_a_corner_the_sound_still_arrives() {
        // One room with a thick wall across most of it: the two sides share their air through a
        // gap. Sealed, the same wall goes all the way.
        let shell = |pos: Pos| {
            let inside = (0..12).contains(&pos[0]) && (0..4).contains(&pos[1]) && (0..8).contains(&pos[2]);
            let around = (-1..=12).contains(&pos[0]) && (-1..=4).contains(&pos[1]) && (-1..=8).contains(&pos[2]);
            (around && !inside).then_some(STONE)
        };
        let gap = |pos: Pos| shell(pos).or_else(|| ((5..=7).contains(&pos[0]) && pos[2] < 6).then_some(STONE));
        let sealed = |pos: Pos| shell(pos).or_else(|| (5..=7).contains(&pos[0]).then_some(STONE));
        let round = evaluate(&gap, [2.5, 1.5, 2.5], [9.5, 1.6, 2.5]);
        let through = evaluate(&sealed, [2.5, 1.5, 2.5], [9.5, 1.6, 2.5]);
        assert!(round.treble > through.treble, "{round:?} against {through:?}");
        assert!(total(&round) > total(&through), "the room's echo reaches the listener through the gap");
    }

    #[test]
    fn gliding_goes_part_of_the_way() {
        let dry = Heard::DRY;
        let wet = Heard { gain: 0.5, treble: 0.0, sends: [1.0, 0.0, 0.5, 0.0] };
        assert_eq!(dry.toward(&wet, 0.0), dry);
        assert_eq!(dry.toward(&wet, 1.0), wet);
        let half = dry.toward(&wet, 0.5);
        assert_eq!((half.gain, half.treble, half.sends), (0.75, 0.5, [0.5, 0.0, 0.25, 0.0]));
    }
}
