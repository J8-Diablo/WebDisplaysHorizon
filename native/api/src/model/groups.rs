//! Linked screens: several screens sharing one link id show one browser picture between
//! them, each its own part.
//!
//! Screens are grouped by link id, dimension, side and rotation. One member, the origin,
//! owns the browser; its link mode and the width of its resolution are the group's.
//! Everything here is data for the caller to apply: nothing touches a browser or draws.
//!
//! A group is laid out on the face, in blocks along the side's right and up, whatever the
//! rotation. JOINED packs the screens as if they touched: each distinct column where a
//! screen starts is as wide as the widest screen starting there, and the columns follow one
//! another without the gaps (rows likewise). Screens that overlap without starting at the
//! same block still get a column each. SPACED keeps the group's bounding box, so the part
//! of the picture that falls in a gap is shown by no screen.
//!
//! The picture is laid over the group's rectangle exactly as a lone screen's is over its
//! own (`hit::face_to_picture`): turned as one piece, and unflipped on the BOTTOM side.
//!
//! Differs from the Java mod on purpose:
//!
//! - The origin election is total and reported. Nothing keeps two screens of a group from
//!   both carrying the origin flag: the lowest position wins (x, then y, then z, then the
//!   order the screens were given in) and the others are named in `Election::Contested`.
//! - The picture's coordinates follow the group's rotation and side. The Java mod turned
//!   each screen of a turned group on its own while offsetting it as if upright, which
//!   scrambled the picture between them, and swapped the rows of a group on a BOTTOM side.
//!   Upright groups on the other sides get the Java mod's numbers.
//! - The signature also covers the maximum resolution, which the group's resolution
//!   depends on.

use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::model::hit::{self, Tile};
use crate::model::resolution;
use crate::model::side::{LinkMode, Pos, Rotation, Side};

/// A screen as the grouping sees it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Member {
    /// The screen's origin block: the one at its lower left on the face. Lays the screen
    /// out in its group and breaks ties between would-be origins.
    pub pos: Pos,
    /// The dimension the screen is in: screens of different worlds never share a picture.
    pub dimension: String,
    /// The face the screen is on.
    pub side: Side,
    /// How its picture is turned.
    pub rotation: Rotation,
    /// Its size in blocks: along the side's right, then along its up.
    pub size: [u32; 2],
    /// Its saved resolution. Only the origin's counts, and of it only the width.
    pub resolution: [u32; 2],
    /// The link it belongs to. Empty for a screen that is not linked.
    pub link_id: String,
    /// How the group shares the picture. Only the origin's counts.
    pub mode: LinkMode,
    /// Whether the screen is flagged as its group's origin, the owner of the browser.
    pub origin: bool,
    /// Which browser the screen shows at the moment, 0 for none: any number that tells two
    /// browsers apart. The layout ignores it; the signature does not, so that a screen left
    /// on another browser than its group's is noticed.
    pub browser: u64,
}

impl Member {
    /// Whether the screen belongs to a link at all.
    pub fn is_linked(&self) -> bool {
        !self.link_id.is_empty()
    }
}

/// What the screens of one group have in common.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GroupKey {
    /// The link the screens share.
    pub link_id: String,
    /// The dimension they are in.
    pub dimension: String,
    /// The face they are on.
    pub side: Side,
    /// How they are all turned.
    pub rotation: Rotation,
}

impl GroupKey {
    /// The group a screen belongs to; none for a screen that is not linked.
    pub fn of(member: &Member) -> Option<GroupKey> {
        member.is_linked().then(|| GroupKey {
            link_id: member.link_id.clone(),
            dimension: member.dimension.clone(),
            side: member.side,
            rotation: member.rotation,
        })
    }
}

/// How a group came by its origin. Screens are named by their index in the slice given to
/// `build`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Election {
    /// Exactly one screen carries the origin flag.
    Flagged,
    /// None does: the screen at the lowest position stands in.
    Unflagged,
    /// Several do: the one at the lowest position won. The `losers` still carry a flag
    /// that means nothing, lowest position first.
    Contested { losers: Vec<usize> },
}

/// One screen's place in its group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    /// The screen: its index in the slice given to `build`.
    pub index: usize,
    /// The part of the group's picture it shows.
    pub tile: Tile,
}

/// The screens of one link on one face, and the picture they share.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// What its screens have in common.
    pub key: GroupKey,
    /// Its screens, lowest position first.
    pub members: Vec<Placed>,
    /// The screen that owns the browser: its index in the slice given to `build`.
    pub origin: usize,
    /// How that screen was chosen.
    pub election: Election,
    /// How the picture is shared: the origin's mode.
    pub mode: LinkMode,
    /// The size of the picture's rectangle in blocks, along the side's right and up.
    pub size: [u32; 2],
    /// The picture's resolution: the origin's width, the height following the group's shape.
    pub resolution: [u32; 2],
    /// What this client's browser renders of it, at its quality and within the maximum.
    pub effective: [u32; 2],
}

impl Group {
    /// The part of the picture a screen shows; none for a screen of another group.
    pub fn tile_of(&self, index: usize) -> Option<Tile> {
        self.members.iter().find(|placed| placed.index == index).map(|placed| placed.tile)
    }

    /// The width and height to give the origin's browser: swapped by a quarter turn.
    pub fn browser_size(&self) -> [u32; 2] {
        resolution::browser_size(self.effective, self.key.rotation)
    }
}

/// The groups the linked screens among `members` form, in a fixed order (by link id, then
/// dimension, side and rotation). `max` is the server's maximum resolution and
/// `quality_percent` this client's quality scale.
pub fn build(members: &[Member], max: [u32; 2], quality_percent: u32) -> Vec<Group> {
    let mut by_key: BTreeMap<(&str, &str, u8, u8), Vec<usize>> = BTreeMap::new();
    for (index, member) in members.iter().enumerate() {
        if member.is_linked() {
            let key = (&*member.link_id, &*member.dimension, member.side.index(), member.rotation.index());
            by_key.entry(key).or_default().push(index);
        }
    }
    by_key.into_values().map(|indices| group_of(members, indices, max, quality_percent)).collect()
}

/// Something that changes whenever `build` would give other groups, or a screen ends up on
/// another browser: the caller rebuilds and shares the browsers out again only then. Cheap
/// enough for every tick, and the order of the screens does not count.
pub fn signature(members: &[Member], max: [u32; 2], quality_percent: u32) -> u64 {
    let settings = hash_of(&(max.map(|side| side.max(1)), quality_percent.clamp(1, 100)));
    members
        .iter()
        .filter(|member| member.is_linked())
        .fold(settings, |total, member| total.wrapping_add(hash_of(member)))
}

/// Where a point of a linked screen falls in the group's picture: u and v, 0 to 1 from the
/// picture's left and from its top, as a texture is addressed. `at` is in blocks from the
/// screen's lower-left corner along the side's right and up; the corners of whole blocks
/// are what a renderer drawing one quad a block asks for.
pub fn point_uv(side: Side, rotation: Rotation, tile: &Tile, at: [f32; 2]) -> [f32; 2] {
    hit::face_to_picture(side, rotation, tile.at(at))
}

/// The picture under one block of a linked screen, `cell` blocks right and up of its
/// lower-left one. For an upright screen that is `(offset + cell) / group size`, V flipped.
pub fn cell_uv(side: Side, rotation: Rotation, tile: &Tile, cell: [u32; 2]) -> Quad {
    rect_uv(side, rotation, tile, cell, [cell[0] + 1, cell[1] + 1])
}

/// The picture under a whole linked screen of `size` blocks.
pub fn screen_uv(side: Side, rotation: Rotation, tile: &Tile, size: [u32; 2]) -> Quad {
    rect_uv(side, rotation, tile, [0, 0], size)
}

/// Where the corners of a rectangle of a screen fall in the group's picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    /// U and V at the rectangle's lower-left, lower-right, upper-right and upper-left
    /// corners, as they are on the face. A turned picture has them in another order than an
    /// upright one, which is why there are four and not a rectangle.
    pub corners: [[f32; 2]; 4],
}

impl Quad {
    /// The rectangle of the picture the corners span: its left, top, right and bottom.
    pub fn bounds(&self) -> [f32; 4] {
        let (us, vs) = (self.corners.map(|corner| corner[0]), self.corners.map(|corner| corner[1]));
        let lowest = |values: [f32; 4]| values.into_iter().fold(f32::INFINITY, f32::min);
        let highest = |values: [f32; 4]| values.into_iter().fold(f32::NEG_INFINITY, f32::max);
        [lowest(us), lowest(vs), highest(us), highest(vs)]
    }
}

fn rect_uv(side: Side, rotation: Rotation, tile: &Tile, from: [u32; 2], to: [u32; 2]) -> Quad {
    let corner = |x: u32, y: u32| point_uv(side, rotation, tile, [x as f32, y as f32]);
    let ([left, bottom], [right, top]) = (from, to);
    Quad { corners: [corner(left, bottom), corner(right, bottom), corner(right, top), corner(left, top)] }
}

fn hash_of(value: &impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// Builds the group of the screens at `indices`, which share a key and are at least one.
fn group_of(members: &[Member], mut indices: Vec<usize>, max: [u32; 2], quality_percent: u32) -> Group {
    indices.sort_by_key(|&index| (members[index].pos, index));
    let (origin, election) = elect(members, &indices);
    let leader = &members[origin];
    let (side, mode) = (leader.side, leader.mode);

    // The two axes are laid out one without the other.
    let spans = |axis: usize, way: Pos| -> Vec<(i64, u32)> {
        indices.iter().map(|&index| (along(way, members[index].pos), members[index].size[axis])).collect()
    };
    let (width, lefts) = lay_out(mode, &spans(0, side.right()));
    let (height, bottoms) = lay_out(mode, &spans(1, side.up()));
    let size = [width, height];

    let resolution = resolution::fit_to_shape(size, leader.resolution[0], max);
    Group {
        key: GroupKey {
            link_id: leader.link_id.clone(),
            dimension: leader.dimension.clone(),
            side,
            rotation: leader.rotation,
        },
        members: indices
            .iter()
            .zip(lefts.iter().zip(&bottoms))
            .map(|(&index, (&left, &bottom))| {
                let tile = Tile { offset: [left, bottom], group: size };
                Placed { index, tile }
            })
            .collect(),
        origin,
        election,
        mode,
        size,
        resolution,
        effective: resolution::effective(resolution, quality_percent, max),
    }
}

/// The origin among `sorted`, lowest position first and not empty: the first flagged one,
/// or the first of all when none is.
fn elect(members: &[Member], sorted: &[usize]) -> (usize, Election) {
    let flagged: Vec<usize> = sorted.iter().copied().filter(|&index| members[index].origin).collect();
    match flagged.as_slice() {
        [] => (sorted[0], Election::Unflagged),
        [only] => (*only, Election::Flagged),
        [winner, losers @ ..] => (*winner, Election::Contested { losers: losers.to_vec() }),
    }
}

/// How far along a direction a block is.
fn along(way: Pos, pos: Pos) -> i64 {
    (0..3).map(|i| way[i] as i64 * pos[i] as i64).sum()
}

/// Lays screens out along one axis, each given as where it starts and how long it is.
/// Gives the group's length and where each screen starts in it.
fn lay_out(mode: LinkMode, spans: &[(i64, u32)]) -> (u32, Vec<u32>) {
    let blocks = |length: i64| u32::try_from(length).unwrap_or(u32::MAX);
    match mode {
        LinkMode::Spaced => {
            let low = spans.iter().map(|&(start, _)| start).min().unwrap_or(0);
            let high = spans.iter().map(|&(start, length)| start + length as i64).max().unwrap_or(0);
            (blocks(high - low), spans.iter().map(|&(start, _)| blocks(start - low)).collect())
        }
        LinkMode::Joined => {
            // The longest screen starting at each distinct place, in order.
            let mut lengths: BTreeMap<i64, u32> = BTreeMap::new();
            for &(start, length) in spans {
                let longest = lengths.entry(start).or_insert(0);
                *longest = (*longest).max(length);
            }
            let mut cursor = 0u32;
            for length in lengths.values_mut() {
                let packed = cursor;
                cursor = cursor.saturating_add(*length);
                *length = packed;
            }
            // `lengths` now holds where each distinct start was packed to.
            (cursor, spans.iter().map(|(start, _)| lengths[start]).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::side::offset;

    const FULL_HD: [u32; 2] = [1920, 1080];

    /// A linked 2x2 screen on a south face.
    fn screen(pos: Pos) -> Member {
        Member {
            pos,
            dimension: "minecraft:overworld".into(),
            side: Side::South,
            rotation: Rotation::R0,
            size: [2, 2],
            resolution: [1920, 1080],
            link_id: "wall".into(),
            mode: LinkMode::Joined,
            origin: false,
            browser: 0,
        }
    }

    fn all(members: &[Member], mode: LinkMode) -> Vec<Member> {
        members.iter().cloned().map(|member| Member { mode, ..member }).collect()
    }

    /// The only group of `members`, at full quality.
    fn only_group(members: &[Member]) -> Group {
        let mut groups = build(members, FULL_HD, 100);
        assert_eq!(groups.len(), 1, "{groups:?}");
        groups.remove(0)
    }

    fn offsets(group: &Group, count: usize) -> Vec<[u32; 2]> {
        (0..count).map(|index| group.tile_of(index).expect("in the group").offset).collect()
    }

    #[test]
    fn two_screens_side_by_side_share_the_width() {
        // On a south face the right is east.
        let members = [screen([0, 0, 0]), screen([2, 0, 0])];
        for mode in [LinkMode::Joined, LinkMode::Spaced] {
            let group = only_group(&all(&members, mode));
            assert_eq!(group.size, [4, 2], "{mode:?}");
            assert_eq!(offsets(&group, 2), [[0, 0], [2, 0]], "{mode:?}");
            assert_eq!(group.tile_of(1), Some(Tile { offset: [2, 0], group: [4, 2] }));
        }
    }

    #[test]
    fn two_stacked_screens_share_the_height() {
        let members = [screen([0, 0, 0]), screen([0, 2, 0])];
        for mode in [LinkMode::Joined, LinkMode::Spaced] {
            let group = only_group(&all(&members, mode));
            assert_eq!(group.size, [2, 4], "{mode:?}");
            assert_eq!(offsets(&group, 2), [[0, 0], [0, 2]], "offsets are counted upwards");
        }
    }

    #[test]
    fn a_gap_is_packed_away_when_joined_and_kept_when_spaced() {
        let members = [screen([0, 0, 0]), screen([5, 0, 0])];
        let joined = only_group(&all(&members, LinkMode::Joined));
        assert_eq!(joined.size, [4, 2]);
        assert_eq!(offsets(&joined, 2), [[0, 0], [2, 0]]);
        let spaced = only_group(&all(&members, LinkMode::Spaced));
        assert_eq!(spaced.size, [7, 2], "three blocks of picture fall in the gap");
        assert_eq!(offsets(&spaced, 2), [[0, 0], [5, 0]]);
    }

    #[test]
    fn a_vertical_gap_is_treated_the_same_way() {
        let members = [screen([0, 0, 0]), screen([0, 6, 0])];
        assert_eq!(only_group(&all(&members, LinkMode::Joined)).size, [2, 4]);
        let spaced = only_group(&all(&members, LinkMode::Spaced));
        assert_eq!(spaced.size, [2, 8]);
        assert_eq!(offsets(&spaced, 2), [[0, 0], [0, 6]]);
    }

    #[test]
    fn three_screens_in_an_l_span_a_square_with_one_corner_unshown() {
        let members = [screen([0, 0, 0]), screen([2, 0, 0]), screen([0, 2, 0])];
        for mode in [LinkMode::Joined, LinkMode::Spaced] {
            let group = only_group(&all(&members, mode));
            assert_eq!(group.size, [4, 4], "{mode:?}");
            assert_eq!(offsets(&group, 3), [[0, 0], [2, 0], [0, 2]], "{mode:?}");
        }
        // Pulled apart, the L keeps its gaps only when spaced.
        let apart = [screen([0, 0, 0]), screen([4, 0, 0]), screen([0, 3, 0])];
        assert_eq!(only_group(&all(&apart, LinkMode::Joined)).size, [4, 4]);
        let spaced = only_group(&all(&apart, LinkMode::Spaced));
        assert_eq!(spaced.size, [6, 5]);
        assert_eq!(offsets(&spaced, 3), [[0, 0], [4, 0], [0, 3]]);
    }

    #[test]
    fn positions_are_measured_along_the_sides_own_right_and_up() {
        // Facing a north face, the right is west.
        let north = |pos| Member { side: Side::North, ..screen(pos) };
        let group = only_group(&[north([0, 0, 0]), north([-2, 0, 0])]);
        assert_eq!(group.size, [4, 2]);
        assert_eq!(offsets(&group, 2), [[0, 0], [2, 0]], "the screen further west is on the right");
        // On a floor's upper face the up is north.
        let top = |pos| Member { side: Side::Top, ..screen(pos) };
        let group = only_group(&[top([0, 0, 0]), top([0, 0, -2])]);
        assert_eq!(group.size, [2, 4]);
        assert_eq!(offsets(&group, 2), [[0, 0], [0, 2]], "the screen further north is above");
    }

    #[test]
    fn joined_gives_each_distinct_start_a_column_as_wide_as_its_widest_screen() {
        let members = [
            screen([0, 0, 0]),
            Member { size: [3, 1], ..screen([0, 2, 0]) },
            Member { size: [1, 2], ..screen([7, 0, 0]) },
        ];
        let group = only_group(&members);
        assert_eq!(group.size, [4, 3], "columns of 3 and 1, rows of 2 and 1");
        assert_eq!(offsets(&group, 3), [[0, 0], [0, 2], [3, 0]]);
    }

    #[test]
    fn with_no_flag_the_lowest_position_is_the_origin() {
        // x counts first, then y, then z.
        let members = [screen([2, 0, 0]), screen([0, 5, 0]), screen([0, 5, -1]), screen([1, 0, 0])];
        let group = only_group(&members);
        assert_eq!(group.origin, 2);
        assert_eq!(group.election, Election::Unflagged);
        let order: Vec<usize> = group.members.iter().map(|placed| placed.index).collect();
        assert_eq!(order, [2, 1, 3, 0], "members are listed lowest position first");
    }

    #[test]
    fn one_flag_makes_the_origin_wherever_it_is() {
        let members = [screen([0, 0, 0]), Member { origin: true, ..screen([2, 0, 0]) }];
        let group = only_group(&members);
        assert_eq!(group.origin, 1);
        assert_eq!(group.election, Election::Flagged);
    }

    #[test]
    fn of_several_flags_the_lowest_position_wins_and_the_others_are_reported() {
        let flagged = |pos| Member { origin: true, ..screen(pos) };
        let members = [flagged([4, 0, 0]), screen([0, 0, 0]), flagged([2, 0, 0]), flagged([6, 0, 0])];
        let group = only_group(&members);
        assert_eq!(group.origin, 2, "the unflagged screen at the lowest position does not count");
        assert_eq!(group.election, Election::Contested { losers: vec![0, 3] });
    }

    #[test]
    fn screens_given_at_one_position_are_told_apart_by_their_order() {
        let members = [screen([0, 0, 0]), screen([0, 0, 0])];
        assert_eq!(only_group(&members).origin, 0);
        let both_flagged = all(&members, LinkMode::Joined).into_iter().map(|m| Member { origin: true, ..m });
        let group = only_group(&both_flagged.collect::<Vec<_>>());
        assert_eq!((group.origin, group.election), (0, Election::Contested { losers: vec![1] }));
    }

    #[test]
    fn the_origin_sets_the_groups_mode_and_resolution() {
        let members = [
            Member { mode: LinkMode::Joined, resolution: [1000, 9], ..screen([0, 0, 0]) },
            Member { mode: LinkMode::Spaced, resolution: [800, 600], origin: true, ..screen([5, 0, 0]) },
        ];
        let group = only_group(&members);
        assert_eq!(group.mode, LinkMode::Spaced);
        assert_eq!(group.size, [7, 2]);
        // 108x28 native: the origin's width, and 800 * 28 / 108 = 207.4 for the height.
        assert_eq!(group.resolution, [800, 207], "the origin's own height does not count");
    }

    #[test]
    fn the_groups_resolution_follows_its_shape_the_quality_and_the_maximum() {
        let wide = [screen([0, 0, 0]), screen([2, 0, 0])];
        // 4x2 blocks are 60x28 native: 1920 * 28 / 60 = 896.
        let group = &build(&wide, FULL_HD, 100)[0];
        assert_eq!((group.resolution, group.effective), ([1920, 896], [1920, 896]));
        let half = &build(&wide, FULL_HD, 50)[0];
        assert_eq!((half.resolution, half.effective), ([1920, 896], [960, 448]));
        assert_eq!(half.browser_size(), [960, 448]);
        let capped = &build(&wide, [1280, 720], 100)[0];
        assert_eq!(capped.resolution, [1280, 597], "1280 * 28 / 60 = 597.3");
        // 2x4 blocks are 28x60 native: too tall for 1920 wide, so 1080 * 28 / 60 = 504 wide.
        let tall = [screen([0, 0, 0]), screen([0, 2, 0])];
        assert_eq!(build(&tall, FULL_HD, 100)[0].resolution, [504, 1080]);
    }

    #[test]
    fn a_quarter_turned_group_gives_its_browser_the_sides_swapped() {
        let turned = |pos| Member { rotation: Rotation::R90, ..screen(pos) };
        let group = &build(&[turned([0, 0, 0]), turned([2, 0, 0])], FULL_HD, 50)[0];
        assert_eq!(group.size, [4, 2], "the layout stays on the face");
        assert_eq!(group.effective, [960, 448]);
        assert_eq!(group.browser_size(), [448, 960]);
    }

    #[test]
    fn screens_that_differ_in_side_rotation_dimension_or_link_are_in_different_groups() {
        let members = [
            screen([0, 0, 0]),
            screen([2, 0, 0]),
            Member { side: Side::North, ..screen([4, 0, 0]) },
            Member { rotation: Rotation::R180, ..screen([6, 0, 0]) },
            Member { dimension: "minecraft:the_nether".into(), ..screen([8, 0, 0]) },
            Member { link_id: "other".into(), ..screen([10, 0, 0]) },
            Member { link_id: String::new(), ..screen([12, 0, 0]) },
        ];
        let groups = build(&members, FULL_HD, 100);
        let mut sets: Vec<Vec<usize>> =
            groups.iter().map(|group| group.members.iter().map(|placed| placed.index).collect()).collect();
        sets.sort();
        assert_eq!(sets, [vec![0, 1], vec![2], vec![3], vec![4], vec![5]], "the unlinked one is in none");
        for group in &groups {
            let first = &members[group.members[0].index];
            assert_eq!(Some(&group.key), GroupKey::of(first).as_ref());
            assert_eq!(group.tile_of(6), None);
        }
        assert_eq!(GroupKey::of(&members[6]), None);
        assert_eq!(build(&members, FULL_HD, 100), groups, "the order of the groups is fixed");
    }

    #[test]
    fn an_upright_block_shows_offset_plus_cell_over_group_size_with_v_flipped() {
        let tile = Tile { offset: [2, 0], group: [4, 2] };
        let lower_left = cell_uv(Side::South, Rotation::R0, &tile, [0, 0]);
        assert_eq!(lower_left.corners, [[0.5, 1.0], [0.75, 1.0], [0.75, 0.5], [0.5, 0.5]]);
        assert_eq!(lower_left.bounds(), [0.5, 0.5, 0.75, 1.0]);
        assert_eq!(cell_uv(Side::South, Rotation::R0, &tile, [1, 1]).bounds(), [0.75, 0.0, 1.0, 0.5]);
        assert_eq!(screen_uv(Side::South, Rotation::R0, &tile, [2, 2]).bounds(), [0.5, 0.0, 1.0, 1.0]);
        assert_eq!(point_uv(Side::South, Rotation::R0, &tile, [0.5, 1.5]), [0.625, 0.25]);
    }

    #[test]
    fn joined_blocks_cover_the_whole_picture_exactly_once_on_every_side_and_rotation() {
        for side in Side::ALL {
            for rotation in Rotation::ALL {
                // Two 2x2 screens with a gap of three blocks along the side's right.
                let first = Member { side, rotation, ..screen([10, 64, -3]) };
                let second = Member { pos: offset(first.pos, side.right(), 5), ..first.clone() };
                let group = only_group(&[first, second]);
                assert_eq!(group.size, [4, 2]);
                // The picture is 4 blocks by 2, or 2 by 4 after a quarter turn.
                let [columns, rows] = if rotation.is_vertical() { [2usize, 4] } else { [4, 2] };
                let mut shown = vec![0u32; columns * rows];
                for placed in &group.members {
                    for cell in [[0, 0], [1, 0], [0, 1], [1, 1]] {
                        let [left, top, right, bottom] = cell_uv(side, rotation, &placed.tile, cell).bounds();
                        let (column, row) = (left * columns as f32, top * rows as f32);
                        let what = format!("{side:?} {rotation:?} {placed:?} {cell:?}");
                        assert_eq!((column.fract(), row.fract()), (0.0, 0.0), "on the grid: {what}");
                        assert_eq!((right - left) * columns as f32, 1.0, "one block wide: {what}");
                        assert_eq!((bottom - top) * rows as f32, 1.0, "one block tall: {what}");
                        shown[row as usize * columns + column as usize] += 1;
                    }
                }
                assert!(shown.iter().all(|&times| times == 1), "{side:?} {rotation:?}: {shown:?}");
            }
        }
    }

    #[test]
    fn a_spaced_gap_takes_its_share_of_the_picture() {
        let members = all(&[screen([0, 0, 0]), screen([5, 0, 0])], LinkMode::Spaced);
        let group = only_group(&members);
        let near = screen_uv(Side::South, Rotation::R0, &group.tile_of(0).unwrap(), [2, 2]).bounds();
        let far = screen_uv(Side::South, Rotation::R0, &group.tile_of(1).unwrap(), [2, 2]).bounds();
        assert_eq!(near, [0.0, 0.0, 2.0 / 7.0, 1.0]);
        assert_eq!(far, [5.0 / 7.0, 0.0, 1.0, 1.0], "nobody shows what is between 2/7 and 5/7");
    }

    #[test]
    fn a_turned_groups_picture_is_turned_as_one_piece() {
        // The upper-left corner of a 4x2 group, seen from the upper-left block of its screen.
        let tile = Tile { offset: [0, 0], group: [4, 2] };
        let corner = |side, rotation| point_uv(side, rotation, &tile, [0.0, 2.0]);
        assert_eq!(corner(Side::South, Rotation::R0), [0.0, 0.0], "the page's top left");
        assert_eq!(corner(Side::South, Rotation::R90), [1.0, 0.0], "its top is at the group's left");
        assert_eq!(corner(Side::South, Rotation::R180), [1.0, 1.0], "upside down");
        assert_eq!(corner(Side::South, Rotation::R270), [0.0, 1.0], "its top is at the group's right");
        assert_eq!(corner(Side::Bottom, Rotation::R0), [0.0, 1.0], "from below, the top is south");
        // Upside down, the left screen of two shows the picture's right half.
        let left = screen_uv(Side::South, Rotation::R180, &tile, [2, 2]);
        assert_eq!(left.bounds(), [0.5, 0.0, 1.0, 1.0]);
        assert_eq!(left.corners[0], [1.0, 0.0], "its lower-left corner is the page's top right");
    }

    #[test]
    fn the_signature_ignores_the_order_of_the_screens_and_the_unlinked_ones() {
        let unlinked = Member { link_id: String::new(), ..screen([9, 9, 9]) };
        let members = [screen([0, 0, 0]), screen([2, 0, 0]), unlinked.clone()];
        let before = signature(&members, FULL_HD, 100);
        assert_eq!(signature(&members, FULL_HD, 100), before, "nothing changed");
        let reordered = [members[2].clone(), members[1].clone(), members[0].clone()];
        assert_eq!(signature(&reordered, FULL_HD, 100), before);
        let other = Member { size: [5, 5], browser: 7, ..unlinked };
        let resized = [members[0].clone(), members[1].clone(), other];
        assert_eq!(signature(&resized, FULL_HD, 100), before);
        assert_eq!(signature(&members[..2], FULL_HD, 100), before, "an unlinked screen less");
        assert_eq!(signature(&members, FULL_HD, 250), before, "a quality above 100 is 100");
    }

    #[test]
    fn the_signature_changes_with_anything_the_groups_depend_on() {
        let members = [screen([0, 0, 0]), screen([2, 0, 0])];
        let before = signature(&members, FULL_HD, 100);
        type Change = fn(&mut Member);
        let changes: [(&str, Change); 10] = [
            ("position", |m| m.pos = [0, 0, 1]),
            ("dimension", |m| m.dimension = "minecraft:the_end".into()),
            ("side", |m| m.side = Side::North),
            ("rotation", |m| m.rotation = Rotation::R90),
            ("size", |m| m.size = [2, 3]),
            ("resolution", |m| m.resolution = [1921, 1049]),
            ("link", |m| m.link_id = "other".into()),
            ("mode", |m| m.mode = LinkMode::Spaced),
            ("origin flag", |m| m.origin = true),
            ("browser", |m| m.browser = 42),
        ];
        for (what, change) in changes {
            let mut changed = members.clone();
            change(&mut changed[0]);
            assert_ne!(signature(&changed, FULL_HD, 100), before, "{what}");
        }
        assert_ne!(signature(&members, FULL_HD, 99), before, "quality");
        assert_ne!(signature(&members, [1920, 1081], 100), before, "maximum");
        assert_ne!(signature(&members[..1], FULL_HD, 100), before, "a screen less");
        let twice = [members[0].clone(), members[1].clone(), members[1].clone()];
        assert_ne!(signature(&twice, FULL_HD, 100), before, "a screen more");
        let unlinked = Member { link_id: String::new(), ..members[1].clone() };
        assert_ne!(signature(&[members[0].clone(), unlinked], FULL_HD, 100), before, "a screen unlinked");
    }
}
