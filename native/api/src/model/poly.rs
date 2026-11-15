//! A screen traced by hand. The player marks block corners with a pen; the closed loop of
//! corners is the screen's outline, and the screen is a thin vertical sheet standing on a
//! horizontal path, its "spine".
//!
//! The sheet is unrolled to take a flat page: `u` is the distance travelled along the spine,
//! `v` the world height. In (u, v) the outline is an ordinary polygon, so the page maps onto
//! it undistorted however the sheet bends; projecting onto one axis instead would squash an
//! oblique run like a shadow.
//!
//! Three parts. [`Outline`] is what is saved and sent. [`Surface`] is the unrolled sheet:
//! built once each time the outline changes, then queried by the renderer every frame and by
//! every click, without allocating. The `plan_*` functions decide which blocks carry the
//! sheet; nothing here writes to a world, a plan is a list of positions the caller applies.
//!
//! What may surprise:
//! - The spine comes from where the corners are, never from the order they were traced in.
//!   The distinct horizontal positions are chained from one end of the farthest-apart pair,
//!   each time to the nearest one not used yet. Following the trace instead adds the return
//!   leg's length to the outbound one's, and a wall with a notch comes out scrambled. The
//!   chaining is greedy: a U whose legs are longer than its mouth is wide gets chained across
//!   the mouth.
//! - A traced sheet has no front of its own. Its normal follows the direction the spine
//!   happens to run in, so half the time it points away from the player: hence `flipped`.
//! - Corners sit on block corners, so the sheet lies on the faces between blocks and in no
//!   cell. One invisible full block per column carries it, on either side: that block has a
//!   face in the sheet's plane, so the screen is hit from both sides without a wall two
//!   blocks thick.
//! - The corner radius is part of the outline. Were each side to read it from its own
//!   settings, a server and a client that disagree would put the blocks and the picture in
//!   different places.
//! - A traced screen has no rotation: neither the page coordinates given here nor the mesh
//!   they describe turn the picture. Whoever adds rotation must turn both.

use std::collections::HashSet;
use std::ops::Deref;

use crate::model::phrase::Phrase;
use crate::model::side::Pos;

/// Fewest corners a loop can close on.
pub const MIN_POINTS: usize = 3;
/// Most corners an outline holds: enough for a smooth curve, few enough to keep meshes and
/// packets small.
pub const MAX_POINTS: usize = 64;
/// Most separate vertical runs of sheet at one place along it: each takes two edges of the
/// outline.
pub const MAX_SPANS: usize = MAX_POINTS / 2;
/// Most nodes a spine has once its corners are rounded. Every query walks the spine, so a
/// shape with many corners gets fewer samples on each.
pub const MAX_SPINE_NODES: usize = 192;

/// How thick the sheet is drawn, in blocks: three pixels, visible from the side.
pub const THICKNESS: f32 = 3.0 / 16.0;

/// Widest sheet that may be placed, measured along the spine, in blocks.
pub const MAX_WIDTH: f32 = 64.0;
/// Tallest sheet that may be placed, in blocks.
pub const MAX_HEIGHT: f32 = 32.0;
/// Most columns of cells a sheet may ask for, so that one trace cannot carpet a region.
pub const MAX_CELLS: usize = 1024;

/// The corner radius of a new outline, in blocks.
pub const DEFAULT_CURVE_RADIUS: f32 = 1.5;
/// The smallest corner radius a player may ask for.
pub const MIN_CURVE_RADIUS: f32 = 0.5;
/// The largest corner radius: anything above counts as this.
pub const MAX_CURVE_RADIUS: f32 = 8.0;

/// How far inside a strip its two edges are sampled. Exactly on a seam an edge of the
/// outline may start or end, and which side it counts for is a coin toss.
pub const SEAM_INSET: f32 = 1.0e-4;

/// Samples on a rounded corner when the spine has room for them.
const CURVE_SAMPLES: usize = 6;
/// Step along the spine when collecting cells: under half a block, so that no cell the
/// sheet crosses is stepped over, even on a shallow diagonal.
const WALK_STEP: f32 = 0.25;
/// How far off the sheet the cell on each side is looked for: a quarter of a block lands
/// well inside the neighbouring cell without reaching the one after.
const SIDE_STEP: f32 = 0.25;
/// How far inside each end of the sheet its first and last columns are looked for: half a
/// step. The cells either side are found a quarter of a block off the sheet, square to
/// it; where a rounded bend leaves the sheet a little askew at its end, a look from any
/// nearer the end would step past it, into a cell the sheet does not reach.
const END_INSET: f32 = WALK_STEP / 2.0;
/// Two positions along the spine closer than this are the same one.
const SAME_U: f32 = 1.0e-4;

/// The block corner nearest to a point of the world: where the pen puts a point when a
/// block is clicked there. A coordinate exactly between two corners goes to the higher.
pub fn nearest_corner(hit: [f64; 3]) -> Pos {
    hit.map(|coordinate| {
        let below = coordinate.floor();
        (if coordinate - below >= 0.5 { below + 1.0 } else { below }) as i32
    })
}

/// The corners a player traced, in the order they were placed, and how the sheet standing
/// on them is shaped and faced. The last corner joins the first.
#[derive(Clone, Debug, PartialEq)]
pub struct Outline {
    points: Vec<Pos>,
    /// Whether the bends of the spine are rounded instead of left sharp. Rounding moves the
    /// sheet (it cuts inside each corner), so the blocks carrying it must be refitted.
    pub curved: bool,
    /// Whether the page is shown on the other face of the sheet. Moves nothing.
    pub flipped: bool,
    /// Radius of a rounded corner, in blocks; none is rounded at 0. Set by the server when
    /// the screen is placed and kept with the outline, so that every side bends the sheet
    /// alike. At each corner it is capped by the walls meeting there (`corner_room`).
    pub curve_radius: f32,
}

impl Default for Outline {
    fn default() -> Self {
        Self { points: Vec::new(), curved: false, flipped: false, curve_radius: DEFAULT_CURVE_RADIUS }
    }
}

impl Outline {
    /// An outline with no corner yet: sharp, not flipped, with the default radius.
    pub fn new() -> Self {
        Self::default()
    }

    /// An outline of these corners, as saved or received; those past `MAX_POINTS` are dropped.
    pub fn from_points(points: impl IntoIterator<Item = Pos>) -> Self {
        Self { points: points.into_iter().take(MAX_POINTS).collect(), ..Self::default() }
    }

    /// The largest corner radius that still changes the sheet: at it, the corner with the
    /// most room has taken all the wall it may. 0 for an outline without a bend.
    pub fn widest_radius(&self) -> f32 {
        let Some(nodes) = chain_spine(&self.points) else {
            return 0.0;
        };
        (0..nodes.len().saturating_sub(2)).map(|index| corner_room(&nodes, index)).fold(0.0, f32::max).min(MAX_CURVE_RADIUS)
    }

    /// The corners, in the order they were placed.
    pub fn points(&self) -> &[Pos] {
        &self.points
    }

    /// How many corners were placed.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Whether no corner was placed yet.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// The corner the loop closes on.
    pub fn first(&self) -> Option<Pos> {
        self.points.first().copied()
    }

    /// The corner the next segment starts from.
    pub fn last(&self) -> Option<Pos> {
        self.points.last().copied()
    }

    /// Whether `MAX_POINTS` is reached: no corner can be added, the loop can only be closed.
    pub fn is_full(&self) -> bool {
        self.points.len() >= MAX_POINTS
    }

    /// Where `corner` is in the outline, if it is one of its points.
    pub fn index_of(&self, corner: Pos) -> Option<usize> {
        self.points.iter().position(|&point| point == corner)
    }

    /// Places a corner after the last. False, and nothing changes, when the outline is full
    /// or `corner` is the one just placed.
    pub fn add(&mut self, corner: Pos) -> bool {
        if self.is_full() || self.last() == Some(corner) {
            return false;
        }
        self.points.push(corner);
        true
    }

    /// Takes one corner out; the two either side of it become neighbours. False when there
    /// is no such corner.
    pub fn remove_at(&mut self, index: usize) -> bool {
        if index >= self.points.len() {
            return false;
        }
        self.points.remove(index);
        true
    }

    /// Forgets every corner. The shape settings stay.
    pub fn clear(&mut self) {
        self.points.clear();
    }

    /// Whether placing `corner` now would close the loop: it is the first corner and there
    /// are enough of them. What turns the pen's preview green.
    pub fn would_close(&self, corner: Pos) -> bool {
        self.points.len() >= MIN_POINTS && self.first() == Some(corner)
    }

    /// +1 or -1: the side of the sheet that shows the page, along the spine's normal.
    pub fn facing(&self) -> f32 {
        if self.flipped { -1.0 } else { 1.0 }
    }

    /// The lowest and the highest corner of the box holding every point; none when empty.
    pub fn bounds(&self) -> Option<[Pos; 2]> {
        let first = self.first()?;
        Some(self.points.iter().fold([first, first], |[low, high], point| {
            [[0, 1, 2].map(|i| low[i].min(point[i])), [0, 1, 2].map(|i| high[i].max(point[i]))]
        }))
    }
}

/// One node of a spine: a horizontal position, and how far along the spine it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpineNode {
    /// World X.
    pub x: f32,
    /// World Z.
    pub z: f32,
    /// Distance travelled along the spine to get here.
    pub u: f32,
}

/// The vertical runs of sheet at one place along it, bottom to top, each as `[low, high]`.
/// A plain value of fixed size, read as a slice.
#[derive(Clone, Copy, Debug)]
pub struct Spans {
    runs: [[f32; 2]; MAX_SPANS],
    len: usize,
}

impl Spans {
    const EMPTY: Spans = Spans { runs: [[0.0; 2]; MAX_SPANS], len: 0 };

    /// The runs, bottom to top.
    pub fn as_slice(&self) -> &[[f32; 2]] {
        &self.runs[..self.len]
    }
}

impl Deref for Spans {
    type Target = [[f32; 2]];

    fn deref(&self) -> &[[f32; 2]] {
        self.as_slice()
    }
}

impl PartialEq for Spans {
    fn eq(&self, other: &Spans) -> bool {
        self.as_slice() == other.as_slice()
    }
}

/// One flat piece of the sheet, between two seams: a trapezoid, since the outline is
/// straight between seams. What the mesh is made of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strip {
    /// The two seams, as distances along the spine.
    pub u: [f32; 2],
    /// Bottom and top of the sheet at the first seam.
    pub left: [f32; 2],
    /// Bottom and top of the sheet at the second seam.
    pub right: [f32; 2],
    /// Whether the first and the second seam are the ends of the whole sheet, where a cap
    /// closes its thickness.
    pub caps: [bool; 2],
}

/// An outline unrolled: the filled sheet, where each point of it is in the world, and which
/// cells it passes. Never changes; build another when the outline does.
#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    /// The spine, at least two nodes, `u` never decreasing.
    spine: Vec<SpineNode>,
    /// The outline's corners as `[u, v]`, in trace order.
    outline: Vec<[f32; 2]>,
    /// Lowest and highest `[u, v]` of the outline.
    low: [f32; 2],
    high: [f32; 2],
    /// Every `u` of a spine node or an outline corner, sorted, repeats included.
    seams: Vec<f32>,
    bounds: [Pos; 2],
    flipped: bool,
}

impl Surface {
    /// Unrolls an outline. None when it bounds no sheet: fewer than `MIN_POINTS` corners,
    /// every corner on one vertical line, or every corner at one height.
    pub fn of(outline: &Outline) -> Option<Surface> {
        let points = outline.points();
        if points.len() < MIN_POINTS {
            return None;
        }
        let corners = chain_spine(points)?;
        let path = round_corners(&corners, if outline.curved { outline.curve_radius } else { 0.0 });

        let mut spine: Vec<SpineNode> = Vec::with_capacity(path.len());
        for &[x, z] in &path {
            let u = spine.last().map_or(0.0, |before| before.u + distance([before.x, before.z], [x, z]));
            spine.push(SpineNode { x, z, u });
        }

        // Each corner takes the `u` of the nearest point of the spine, not of a node at its
        // own position: a rounded bend has cut that position away.
        let unroll = |p: &Pos| [nearest_u(&spine, p[0] as f32 as f64, p[2] as f32 as f64), p[1] as f32];
        let flat: Vec<[f32; 2]> = points.iter().map(unroll).collect();
        let low = flat.iter().fold([f32::MAX; 2], |low, p| [low[0].min(p[0]), low[1].min(p[1])]);
        let high = flat.iter().fold([f32::MIN; 2], |high, p| [high[0].max(p[0]), high[1].max(p[1])]);
        let (width, height) = (high[0] - low[0], high[1] - low[1]);
        // Written so that NaN is refused too.
        if !(width > 0.0 && height > 0.0) {
            return None;
        }

        let mut seams: Vec<f32> = Vec::with_capacity(spine.len() + flat.len() + 2);
        seams.extend([low[0], high[0]]);
        seams.extend(spine.iter().map(|node| node.u));
        seams.extend(flat.iter().map(|p| p[0]));
        seams.sort_unstable_by(f32::total_cmp);

        let (bounds, flipped) = (outline.bounds()?, outline.flipped);
        Some(Surface { spine, outline: flat, low, high, seams, bounds, flipped })
    }

    /// Length of the sheet along its spine, in blocks.
    pub fn width(&self) -> f32 {
        self.high[0] - self.low[0]
    }

    /// Height of the sheet, in blocks.
    pub fn height(&self) -> f32 {
        self.high[1] - self.low[1]
    }

    /// Where the sheet starts along its spine.
    pub fn min_u(&self) -> f32 {
        self.low[0]
    }

    /// Where the sheet ends along its spine.
    pub fn max_u(&self) -> f32 {
        self.high[0]
    }

    /// The world height of the sheet's lowest point.
    pub fn min_v(&self) -> f32 {
        self.low[1]
    }

    /// The world height of the sheet's highest point.
    pub fn max_v(&self) -> f32 {
        self.high[1]
    }

    /// Whether the page is on the face away from the spine's normal.
    pub fn flipped(&self) -> bool {
        self.flipped
    }

    /// +1 or -1: which way the page looks, along `normal_at`.
    pub fn facing(&self) -> f32 {
        if self.flipped { -1.0 } else { 1.0 }
    }

    /// The size a screen of this shape has, in whole blocks, at least one each way: the
    /// unrolled extent, which is what resolution and aspect ratio are worked out from.
    pub fn size_in_blocks(&self) -> [u32; 2] {
        [self.width(), self.height()].map(|extent| (extent.round() as u32).max(1))
    }

    /// The lowest and the highest corner of the box holding the outline. Grown by a block
    /// each way it holds the whole sheet and every cell carrying it.
    pub fn block_bounds(&self) -> [Pos; 2] {
        self.bounds
    }

    /// The path the sheet stands on, from `u` = 0 to its full length.
    pub fn spine(&self) -> &[SpineNode] {
        &self.spine
    }

    /// The outline's corners as `[u, v]`, in the order they were traced.
    pub fn outline_uv(&self) -> &[[f32; 2]] {
        &self.outline
    }

    /// The vertical runs of sheet at `u`: where a vertical line there is inside the outline,
    /// by the even-odd rule, so a notch, an arch or a hole needs no special case. Empty
    /// where the sheet has nothing.
    pub fn spans_at(&self, u: f32) -> Spans {
        let corners = &self.outline;
        let mut hits = [0.0f32; MAX_POINTS];
        let mut count = 0;
        for (i, &[u0, v0]) in corners.iter().enumerate() {
            let [u1, v1] = corners[(i + 1) % corners.len()];
            // Half-open, so a corner counts for exactly one of its two edges; an edge
            // parallel to the line (u0 == u1) never counts.
            let straddles = (u0 <= u && u1 > u) || (u1 <= u && u0 > u);
            if straddles {
                hits[count] = v0 + (v1 - v0) * ((u - u0) / (u1 - u0));
                count += 1;
            }
        }
        if count < 2 {
            return Spans::EMPTY;
        }
        hits[..count].sort_unstable_by(f32::total_cmp);

        // An odd crossing left over at the top pairs with nothing.
        let (runs, _) = hits[..count].as_chunks::<2>();
        let mut spans = Spans::EMPTY;
        spans.runs[..runs.len()].copy_from_slice(runs);
        spans.len = runs.len();
        spans
    }

    /// Whether the sheet has material at that point of the unrolled plane, edges included.
    pub fn contains(&self, u: f32, v: f32) -> bool {
        self.spans_at(u).iter().any(|run| v >= run[0] && v <= run[1])
    }

    /// The horizontal position `[x, z]` of the spine at `u`; its nearer end beyond either.
    pub fn xz_at(&self, u: f32) -> [f32; 2] {
        let (first, last) = (self.spine[0], self.spine[self.spine.len() - 1]);
        if u <= first.u {
            return [first.x, first.z];
        }
        if u >= last.u {
            return [last.x, last.z];
        }
        let segment = self.segment_at(u, false);
        let (from, to) = (self.spine[segment], self.spine[segment + 1]);
        let length = to.u - from.u;
        let along = if length <= 0.0 { 0.0 } else { (u - from.u) / length };
        [from.x + (to.x - from.x) * along, from.z + (to.z - from.z) * along]
    }

    /// The world position `[x, y, z]` of an unrolled point, in the sheet's middle plane.
    pub fn to_world(&self, u: f32, v: f32) -> [f32; 3] {
        let [x, z] = self.xz_at(u);
        [x, v, z]
    }

    /// The sheet's horizontal unit normal `[x, z]` at `u`: the spine's direction turned a
    /// quarter. At a bend, that of the segment arriving there.
    pub fn normal_at(&self, u: f32) -> [f32; 2] {
        self.segment_normal(self.segment_at(u, false))
    }

    /// The horizontal step `[x, z]` from the middle plane at `u` to the face of a sheet
    /// `dist` thick on that side (negative for the other side).
    ///
    /// At a bend this is the mitre of the two segments meeting there, lengthened so that
    /// the face stays parallel to the sheet. Each strip stepping along its own normal would
    /// end at a different point from where the next begins, and tear a wedge open at every
    /// corner. The lengthening stops at four times `dist`, for a fold close to a half turn.
    pub fn offset_at(&self, u: f32, dist: f32) -> [f32; 2] {
        let last = self.spine.len() - 1;
        // The first node within SAME_U of `u`, if there is one.
        let near = self.spine.partition_point(|node| u - node.u >= SAME_U);
        let node = (near <= last && (u - self.spine[near].u).abs() < SAME_U).then_some(near);

        let segment = match node {
            Some(bend) if bend > 0 && bend < last => {
                let (before, after) = (self.segment_normal(bend - 1), self.segment_normal(bend));
                let mitre = [before[0] + after[0], before[1] + after[1]];
                let length = (mitre[0] * mitre[0] + mitre[1] * mitre[1]).sqrt();
                if length >= 1.0e-5 {
                    let mitre = [mitre[0] / length, mitre[1] / length];
                    let cos = mitre[0] * after[0] + mitre[1] * after[1];
                    let scale = dist / cos.max(0.25);
                    return [mitre[0] * scale, mitre[1] * scale];
                }
                // The path doubles straight back on itself: there is no mitre to speak of.
                bend
            }
            Some(end) => end.min(last - 1),
            None => self.segment_at(u, true),
        };
        let normal = self.segment_normal(segment);
        [normal[0] * dist, normal[1] * dist]
    }

    /// How far along the spine the point of it nearest to a horizontal position is.
    pub fn nearest_u(&self, x: f64, z: f64) -> f32 {
        nearest_u(&self.spine, x, z)
    }

    /// Where a point of the world lands on the sheet, as `[u, v]`: the horizontal part goes
    /// to the nearest point of the spine, the height straight through. None when that is
    /// off the filled shape, as when the block carrying the sheet is hit beside it.
    pub fn from_world(&self, x: f64, y: f64, z: f64) -> Option<[f32; 2]> {
        let (u, v) = (self.nearest_u(x, z), y as f32);
        self.contains(u, v).then_some([u, v])
    }

    /// Where an unrolled point is across the whole shape, as texture coordinates: 0 to 1
    /// along the spine, and 0 at the top to 1 at the bottom. For the panel behind the page,
    /// which is never mirrored.
    pub fn panel_uv(&self, u: f32, v: f32) -> [f32; 2] {
        [(u - self.low[0]) / self.width(), 1.0 - (v - self.low[1]) / self.height()]
    }

    /// Where an unrolled point is on the page stretched over the whole shape, as texture
    /// coordinates, 0 at the top. Mirrored along the spine on a flipped screen: seen from
    /// the other face the two ends of the sheet swap sides, over the full width and not
    /// strip by strip. The picture and the clicks both go through here, so they agree.
    pub fn page_uv(&self, u: f32, v: f32) -> [f32; 2] {
        let [across, down] = self.panel_uv(u, v);
        [if self.flipped { 1.0 - across } else { across }, down]
    }

    /// The pixel of a page of `resolution` at an unrolled point, kept on the page. The
    /// screen's rotation plays no part: a traced screen is drawn upright.
    pub fn to_pixels(&self, u: f32, v: f32, resolution: [u32; 2]) -> [u32; 2] {
        let at = self.page_uv(u, v);
        [0, 1].map(|i| {
            let pixel = (at[i] * resolution[i] as f32).round() as i64;
            pixel.clamp(0, (resolution[i] as i64 - 1).max(0)) as u32
        })
    }

    /// The places along the sheet where the mesh needs a seam, in order, none repeated:
    /// spine nodes (a quad across a bend would cut the corner off), outline corners (between
    /// two seams the edge is taken to be straight) and whole blocks.
    pub fn breakpoints(&self) -> Breakpoints<'_> {
        let (first_block, end) = (self.low[0].ceil(), self.high[0]);
        Breakpoints { seams: &self.seams, first_block, blocks: 0, end, last: None }
    }

    /// The sheet cut into flat pieces: for each pair of neighbouring seams, one trapezoid
    /// per vertical run. The edges are read just inside both seams, so a sloped edge of the
    /// outline is followed instead of stepped.
    pub fn strips(&self) -> Strips<'_> {
        Strips {
            surface: self,
            seams: self.breakpoints(),
            from: None,
            u: [0.0; 2],
            left: Spans::EMPTY,
            right: Spans::EMPTY,
            run: 0,
        }
    }

    /// For each column of the sheet, at each height, the two cells either side of it, as
    /// `[preferred, alternate]`; a pair is listed once whichever way round it comes up. The
    /// block carrying the sheet there goes in the first that can take it.
    ///
    /// Allocates, and walks the whole sheet a quarter of a block at a time: for placing,
    /// refitting and removing a sheet within the limits, not for every frame.
    pub fn cell_candidates(&self) -> Vec<[Pos; 2]> {
        let mut pairs: Vec<[Pos; 2]> = Vec::new();
        // Keyed on the pair whichever way round: a zigzag can come back to a preferred cell
        // from another direction with a different neighbour, which is another column, while
        // the normal changing sign round a bend shows one column twice.
        let mut seen: HashSet<[Pos; 2]> = HashSet::new();
        let (start, end) = (self.low[0], self.high[0]);

        // Looked at in the middle of each step, never on a whole step: a wall's end, a
        // sharp bend and the line between two blocks all lie on whole steps, where the cell
        // found is the next one along, which the sheet does not reach. (Walked on whole
        // steps, a wall traced towards -X or -Z was given a column of blocks past its first
        // corner, and a sharp bend one past its corner.) A last look half a step inside
        // the far end, for a sheet whose length is not a whole number of steps.
        let middles = (0u64..).map(|step| start + (step as f32 + 0.5) * WALK_STEP).take_while(|&u| u < end);
        let last = (end - END_INSET).max((start + end) / 2.0);
        for at in middles.chain(std::iter::once(last)) {
            let spans = self.spans_at(at);
            if spans.is_empty() {
                continue;
            }
            let (normal, middle) = (self.normal_at(at), self.xz_at(at));
            let side = |step: f32| [0, 1].map(|i| (middle[i] + normal[i] * step).floor() as i32);
            let (preferred, alternate) = (side(SIDE_STEP), side(-SIDE_STEP));

            for run in spans.iter() {
                for y in run[0].floor() as i32..run[1].ceil() as i32 {
                    let pair = [[preferred[0], y, preferred[1]], [alternate[0], y, alternate[1]]];
                    let key = if pair[0] <= pair[1] { pair } else { [pair[1], pair[0]] };
                    if seen.insert(key) {
                        pairs.push(pair);
                    }
                }
            }
        }
        pairs
    }

    /// The segment `u` is on, by the index of its first node. A `u` exactly on a node goes
    /// to the segment before it, or to the one after when `node_goes_after`.
    fn segment_at(&self, u: f32, node_goes_after: bool) -> usize {
        let inner = &self.spine[1..self.spine.len() - 1];
        inner.partition_point(|node| if node_goes_after { node.u <= u } else { node.u < u })
    }

    /// The unit normal of one segment; that of a segment without length points along +Z.
    fn segment_normal(&self, segment: usize) -> [f32; 2] {
        let (from, to) = (self.spine[segment], self.spine[segment + 1]);
        let (dx, dz) = (to.x - from.x, to.z - from.z);
        let length = distance([from.x, from.z], [to.x, to.z]);
        if length <= 0.0 { [0.0, 1.0] } else { [-dz / length, dx / length] }
    }
}

/// The seams of a sheet in order; see [`Surface::breakpoints`].
#[derive(Clone, Debug)]
pub struct Breakpoints<'a> {
    /// Spine nodes and outline corners, sorted.
    seams: &'a [f32],
    /// The first whole block at or after the start of the sheet, and how many were given.
    first_block: f32,
    blocks: u64,
    /// Whole blocks stop before this.
    end: f32,
    last: Option<f32>,
}

impl Iterator for Breakpoints<'_> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        loop {
            let seam = self.seams.first().copied();
            let block = self.first_block + self.blocks as f32;
            let block = (block < self.end).then_some(block);
            // The lower of the next seam and the next whole block.
            let u = match (seam, block) {
                (Some(seam), Some(block)) if block < seam => block,
                (Some(seam), _) => seam,
                (None, Some(block)) => block,
                (None, None) => return None,
            };
            if seam == Some(u) {
                self.seams = &self.seams[1..];
            } else {
                self.blocks += 1;
            }
            if self.last.is_none_or(|last| (u - last).abs() >= SAME_U) {
                self.last = Some(u);
                return Some(u);
            }
        }
    }
}

/// The flat pieces of a sheet, from the start of the spine to its end and bottom to top;
/// see [`Surface::strips`].
#[derive(Clone, Debug)]
pub struct Strips<'a> {
    surface: &'a Surface,
    seams: Breakpoints<'a>,
    /// The seam the next pair starts at.
    from: Option<f32>,
    /// The pair of seams being handed out, and the runs at each.
    u: [f32; 2],
    left: Spans,
    right: Spans,
    /// The next run of that pair to hand out.
    run: usize,
}

impl Iterator for Strips<'_> {
    type Item = Strip;

    fn next(&mut self) -> Option<Strip> {
        loop {
            if let (Some(&left), Some(&right)) = (self.left.get(self.run), self.right.get(self.run)) {
                self.run += 1;
                let ends = [self.surface.min_u(), self.surface.max_u()];
                let caps = [0, 1].map(|i| (self.u[i] - ends[i]).abs() < SEAM_INSET);
                return Some(Strip { u: self.u, left, right, caps });
            }

            let to = self.seams.next()?;
            let Some(from) = self.from.replace(to) else { continue };
            self.u = [from, to];
            self.run = 0;
            if to - from <= SEAM_INSET {
                (self.left, self.right) = (Spans::EMPTY, Spans::EMPTY);
                continue;
            }
            self.left = self.surface.spans_at(from + SEAM_INSET);
            self.right = self.surface.spans_at(to - SEAM_INSET);
            // Runs that do not pair up mean an edge starts inside the strip, which the seams
            // are there to prevent: a flat strip is better than runs paired wrongly.
            if self.right.len() != self.left.len() {
                self.right = self.left;
            }
        }
    }
}

/// The distinct horizontal positions of an outline, chained into a path: from one end of
/// the farthest-apart pair, each time to the nearest one not used yet. None when the
/// outline has no horizontal extent.
fn chain_spine(points: &[Pos]) -> Option<Vec<[i32; 2]>> {
    let mut distinct: Vec<[i32; 2]> = Vec::with_capacity(points.len());
    for point in points {
        let position = [point[0], point[2]];
        if !distinct.contains(&position) {
            distinct.push(position);
        }
    }
    if distinct.len() < 2 {
        return None;
    }
    // Wide enough that no two block positions overflow it.
    let distance2 = |a: [i32; 2], b: [i32; 2]| {
        let (dx, dz) = (a[0] as i128 - b[0] as i128, a[1] as i128 - b[1] as i128);
        dx * dx + dz * dz
    };

    let mut start = 0;
    let mut farthest = -1;
    for (i, &a) in distinct.iter().enumerate() {
        for &b in &distinct[i + 1..] {
            let apart = distance2(a, b);
            if apart > farthest {
                farthest = apart;
                start = i;
            }
        }
    }

    let mut used = vec![false; distinct.len()];
    let mut path = Vec::with_capacity(distinct.len());
    let mut current = distinct[start];
    used[start] = true;
    path.push(current);
    for _ in 1..distinct.len() {
        // Of several equally near, the one traced first.
        let unused = (0..distinct.len()).filter(|&i| !used[i]);
        let Some(next) = unused.min_by_key(|&i| distance2(current, distinct[i])) else { break };
        used[next] = true;
        current = distinct[next];
        path.push(current);
    }
    Some(path)
}

/// The length of the horizontal step from `a` to `b`.
fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
    (dx * dx + dz * dz).sqrt()
}

/// The spine through the corners of `nodes`, each inner one replaced by a curve of `radius`
/// tangent to both walls meeting there: a quadratic Bezier whose control point is the
/// corner. Nothing is rounded without a radius.
///
/// The radius is fixed, not a share of the walls, so a corner between two long walls rounds
/// as tightly as one between two short ones. It is capped by the room the two walls leave
/// (`corner_room`).
fn round_corners(nodes: &[[i32; 2]], radius: f32) -> Vec<[f32; 2]> {
    let float = |node: &[i32; 2]| [node[0] as f32, node[1] as f32];
    if nodes.len() < 3 || radius.is_nan() || radius <= 0.0 {
        return nodes.iter().map(float).collect();
    }
    let radius = radius.min(MAX_CURVE_RADIUS);
    let corners = nodes.len() - 2;
    let room = MAX_SPINE_NODES.saturating_sub(nodes.len()) / corners;
    let samples = room.saturating_sub(1).clamp(2, CURVE_SAMPLES);

    let mut path = Vec::with_capacity(nodes.len() + corners * (samples + 1));
    path.push(float(&nodes[0]));
    for (index, trio) in nodes.windows(3).enumerate() {
        let (before, corner, after) = (float(&trio[0]), float(&trio[1]), float(&trio[2]));
        let cut = radius.min(corner_room(nodes, index));
        if cut <= 1.0e-4 {
            path.push(corner);
            continue;
        }
        // Where the curve leaves one wall and joins the other.
        let toward = |wall: [f32; 2]| {
            let length = distance(corner, wall);
            [0, 1].map(|i| corner[i] + (wall[i] - corner[i]) / length * cut)
        };
        let (enter, leave) = (toward(before), toward(after));
        for sample in 0..=samples {
            let t = sample as f32 / samples as f32;
            let weights = [(1.0 - t) * (1.0 - t), 2.0 * t * (1.0 - t), t * t];
            path.push([0, 1].map(|i| weights[0] * enter[i] + weights[1] * corner[i] + weights[2] * leave[i]));
        }
    }
    path.push(float(&nodes[nodes.len() - 1]));
    // Two corners sharing a wall no longer than twice the radius end their curves on the
    // same point, its middle. Kept twice, that point would be a segment without length or
    // direction, and the thickness of the sheet is mitred from the directions at each node.
    path.dedup_by(|point, kept| distance(*kept, *point) < SAME_U);
    path
}

/// How much of the two walls meeting at the corner `nodes[index + 1]` its curve may take.
/// A wall between two corners is shared by their curves, half each, so that they cannot
/// swallow the straight run between them. The first and the last wall have one corner
/// only, which may take all of them.
fn corner_room(nodes: &[[i32; 2]], index: usize) -> f32 {
    let float = |node: &[i32; 2]| [node[0] as f32, node[1] as f32];
    let (before, corner, after) = (float(&nodes[index]), float(&nodes[index + 1]), float(&nodes[index + 2]));
    let share = |shared: bool| if shared { 0.5 } else { 1.0 };
    (distance(corner, before) * share(index > 0)).min(distance(corner, after) * share(index + 3 < nodes.len()))
}

/// How far along `spine` the point of it nearest to a horizontal position is. A position
/// past the end of a segment belongs to its tip, not to the line carried on.
fn nearest_u(spine: &[SpineNode], x: f64, z: f64) -> f32 {
    let mut nearest2 = f64::MAX;
    let mut nearest_u = spine[0].u;
    for segment in spine.windows(2) {
        let (from, to) = (segment[0], segment[1]);
        let (ax, az) = (from.x as f64, from.z as f64);
        let (dx, dz) = (to.x as f64 - ax, to.z as f64 - az);
        let length2 = dx * dx + dz * dz;
        let along = if length2 <= 0.0 { 0.0 } else { ((x - ax) * dx + (z - az) * dz) / length2 };
        let along = along.clamp(0.0, 1.0);
        let (qx, qz) = (ax + dx * along, az + dz * along);
        let distance2 = (x - qx) * (x - qx) + (z - qz) * (z - qz);
        if distance2 < nearest2 {
            nearest2 = distance2;
            nearest_u = (from.u as f64 + (to.u - from.u) as f64 * along) as f32;
        }
    }
    nearest_u
}

/// What the planner needs to know of the world.
pub trait Cells {
    /// Whether a block may be put in this cell: it holds air, or something as flimsy as
    /// grass that gives way.
    fn is_free(&self, pos: Pos) -> bool;
}

impl<F: Fn(Pos) -> bool> Cells for F {
    fn is_free(&self, pos: Pos) -> bool {
        self(pos)
    }
}

/// Why a traced screen was not placed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Refusal {
    /// The outline bounds no sheet, or the sheet passes through no cell.
    Degenerate,
    /// Longer along its spine than `MAX_WIDTH`.
    TooWide,
    /// Higher than `MAX_HEIGHT`.
    TooTall,
    /// More columns of cells than `MAX_CELLS`.
    TooManyCells,
    /// Somewhere along the sheet the cells on both sides are taken.
    Obstructed,
}

impl Refusal {
    /// The reason in a few words, as it is told between brackets after "that outline
    /// cannot be a screen".
    pub fn reason(self) -> Phrase {
        match self {
            Refusal::Degenerate => Phrase::new("webdisplays.h.poly.reason.degenerate", "degenerate"),
            Refusal::TooWide => Phrase::new("webdisplays.h.poly.reason.too_wide", "too wide"),
            Refusal::TooTall => Phrase::new("webdisplays.h.poly.reason.too_tall", "too tall"),
            Refusal::TooManyCells => Phrase::new("webdisplays.h.poly.reason.too_many_cells", "too many cells"),
            Refusal::Obstructed => Phrase::new("webdisplays.h.poly.reason.obstructed", "obstructed"),
        }
    }
}

/// Where a traced screen goes: decided whole before anything is written, so a refusal
/// leaves no half screen behind.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildPlan {
    /// Every cell that gets a block, each once, the master first.
    pub cells: Vec<Pos>,
    /// The cell that owns the outline and the screen itself: the first of `cells`. The
    /// others only remember where it is.
    pub master: Pos,
    /// The size the screen is given, in blocks: see [`Surface::size_in_blocks`].
    pub size: [u32; 2],
    /// The sheet the plan was made for, for the master to keep.
    pub surface: Surface,
}

/// Decides where the screen traced by `outline` goes. Each column takes the first of its
/// two cells that is free, so a sheet traced flush against a wall uses the open side.
pub fn plan_build(outline: &Outline, world: &impl Cells) -> Result<BuildPlan, Refusal> {
    let surface = Surface::of(outline).ok_or(Refusal::Degenerate)?;
    if surface.width() > MAX_WIDTH {
        return Err(Refusal::TooWide);
    }
    if surface.height() > MAX_HEIGHT {
        return Err(Refusal::TooTall);
    }
    let candidates = surface.cell_candidates();
    if candidates.len() > MAX_CELLS {
        return Err(Refusal::TooManyCells);
    }

    // Two columns meeting at a bend can settle on the same cell: it is listed once.
    let mut cells = Vec::with_capacity(candidates.len());
    let mut chosen = HashSet::with_capacity(candidates.len());
    for pair in candidates {
        let cell = pair.into_iter().find(|&cell| world.is_free(cell)).ok_or(Refusal::Obstructed)?;
        if chosen.insert(cell) {
            cells.push(cell);
        }
    }
    let master = *cells.first().ok_or(Refusal::Degenerate)?;
    Ok(BuildPlan { cells, master, size: surface.size_in_blocks(), surface })
}

/// How the blocks of a placed screen move after its shape changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefitPlan {
    /// Cells of this screen the sheet no longer passes: to empty first. These are moves,
    /// not breaks: emptying them must not bring the screen down.
    pub remove: Vec<Pos>,
    /// Free cells the sheet now needs: each gets a block that remembers the master.
    pub add: Vec<Pos>,
}

/// Decides how the blocks of the screen owned by `master` follow its sheet after the
/// outline changed, as when its corners are rounded. `owned` tells this screen's cells (its
/// master included) from everything else.
///
/// The screen keeps its master, and with it its page and its settings, even where the sheet
/// has moved off that cell. A column whose two cells are both taken by something else is
/// left out: the screen cannot be clicked there, which is better than refusing the change.
/// Another screen's cells are neither taken over nor emptied. The limits of `plan_build`
/// are not checked again.
pub fn plan_refit(
    surface: &Surface,
    master: Pos,
    world: &impl Cells,
    owned: impl Fn(Pos) -> bool,
) -> RefitPlan {
    let mut wanted: HashSet<Pos> = HashSet::from([master]);
    let mut add = Vec::new();
    for pair in surface.cell_candidates() {
        let Some(cell) = pair.into_iter().find(|&cell| owned(cell) || world.is_free(cell)) else { continue };
        if wanted.insert(cell) && !owned(cell) {
            add.push(cell);
        }
    }

    // The sheet was inside the outline's box before the change as well, so looking one
    // block around that box finds this screen's cells wherever the shape used to run.
    let [low, high] = surface.block_bounds();
    let around = |axis: usize| low[axis].saturating_sub(1)..=high[axis].saturating_add(1);
    let mut remove = Vec::new();
    for x in around(0) {
        for y in around(1) {
            for z in around(2) {
                let cell = [x, y, z];
                if !wanted.contains(&cell) && owned(cell) {
                    remove.push(cell);
                }
            }
        }
    }
    RefitPlan { remove, add }
}

/// The cells to empty to remove the screen owned by `master`: those of its sheet that
/// `owned` says are this screen's, then the master itself, each once. `already_gone` is a
/// cell the game is removing at this very moment, which must not be emptied again.
///
/// A sheet shares the cells along its edge with whatever stands next to it, another traced
/// screen included: only the owner test keeps a neighbour's blocks standing. A master whose
/// outline unrolls to no surface has only itself to remove.
pub fn plan_demolish(
    surface: &Surface,
    master: Pos,
    already_gone: Option<Pos>,
    owned: impl Fn(Pos) -> bool,
) -> Vec<Pos> {
    let mut listed: HashSet<Pos> = HashSet::from([master]);
    let mut cells = Vec::new();
    for cell in surface.cell_candidates().into_iter().flatten() {
        if Some(cell) != already_gone && owned(cell) && listed.insert(cell) {
            cells.push(cell);
        }
    }
    // A screen can own cells its sheet no longer names: built by a version that put a
    // column past a wall's end, or left where a refit could not reach. They are within a
    // block of the outline's box, and go with the screen instead of staying behind unseen.
    let [low, high] = surface.block_bounds();
    let around = |axis: usize| low[axis].saturating_sub(1)..=high[axis].saturating_add(1);
    for x in around(0) {
        for y in around(1) {
            for z in around(2) {
                let cell = [x, y, z];
                if Some(cell) != already_gone && !listed.contains(&cell) && owned(cell) {
                    listed.insert(cell);
                    cells.push(cell);
                }
            }
        }
    }
    if Some(master) != already_gone {
        cells.push(master);
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashMap};

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn close2(a: [f32; 2], b: [f32; 2]) -> bool {
        close(a[0], b[0]) && close(a[1], b[1])
    }

    /// A sheet of one height standing on `path`: out along the bottom, back along the top.
    fn sheet(path: &[[i32; 2]], bottom: i32, top: i32) -> Outline {
        let out = path.iter().map(|&[x, z]| [x, bottom, z]);
        let back = path.iter().rev().map(|&[x, z]| [x, top, z]);
        Outline::from_points(out.chain(back))
    }

    /// A wall along X at `z`, from x = 0 to `length` and from y = 0 to `height`.
    fn wall(length: i32, height: i32, z: i32) -> Outline {
        sheet(&[[0, z], [length, z]], 0, height)
    }

    /// An outline lying in the plane z = 0, from its corners as `[x, y]`.
    fn drawn(corners: &[[i32; 2]]) -> Outline {
        Outline::from_points(corners.iter().map(|&[x, y]| [x, y, 0]))
    }

    fn rounded(mut outline: Outline, radius: f32) -> Outline {
        outline.curved = true;
        outline.curve_radius = radius;
        outline
    }

    fn surface(outline: &Outline) -> Surface {
        Surface::of(outline).expect("this outline bounds a sheet")
    }

    fn spine_xz(surface: &Surface) -> Vec<[f32; 2]> {
        surface.spine().iter().map(|node| [node.x, node.z]).collect()
    }

    /// Two walls of 3 meeting at a right angle.
    const L: [[i32; 2]; 3] = [[0, 0], [3, 0], [3, 3]];
    /// A U whose mouth (4) is wider than its legs are long (2).
    const U: [[i32; 2]; 4] = [[0, 0], [0, 2], [4, 2], [4, 0]];
    /// Three walls of 3 in a Z; the middle one runs along -X.
    const Z: [[i32; 2]; 4] = [[3, 0], [3, 3], [0, 3], [0, 6]];
    /// A wall 10 long and 4 high with a notch 4 wide and 2 deep cut out of its top, as `[x, y]`.
    const NOTCHED: [[i32; 2]; 8] = [[0, 0], [10, 0], [10, 4], [7, 4], [7, 2], [3, 2], [3, 4], [0, 4]];
    /// A wall 6 long and 4 high with a slot cut in from its far end, as `[x, y]`.
    const SLOTTED: [[i32; 2]; 8] = [[0, 0], [6, 0], [6, 1], [2, 1], [2, 3], [6, 3], [6, 4], [0, 4]];

    /// A world of blocks: which screen each belongs to, 0 for a block that is no screen's.
    #[derive(Default)]
    struct World {
        blocks: HashMap<Pos, u32>,
    }

    impl Cells for World {
        fn is_free(&self, pos: Pos) -> bool {
            !self.blocks.contains_key(&pos)
        }
    }

    impl World {
        fn fill(&mut self, cells: &[Pos], screen: u32) {
            self.blocks.extend(cells.iter().map(|&cell| (cell, screen)));
        }

        fn clear(&mut self, cells: &[Pos]) {
            for cell in cells {
                self.blocks.remove(cell);
            }
        }

        fn owned_by(&self, screen: u32) -> impl Fn(Pos) -> bool + '_ {
            move |pos| self.blocks.get(&pos) == Some(&screen)
        }

        fn cells_of(&self, screen: u32) -> BTreeSet<Pos> {
            self.blocks.iter().filter(|&(_, &owner)| owner == screen).map(|(&cell, _)| cell).collect()
        }
    }

    #[test]
    fn the_pen_puts_its_point_on_the_nearest_block_corner() {
        assert_eq!(nearest_corner([1.4, 64.6, -0.4]), [1, 65, 0]);
        assert_eq!(nearest_corner([-1.6, -0.2, 7.49]), [-2, 0, 7]);
        assert_eq!(nearest_corner([0.5, -0.5, -1.5]), [1, 0, -1], "a half goes to the higher corner");
        assert_eq!(nearest_corner([0.49999999999999994, 2.5, -3.0]), [0, 3, -3]);
    }

    #[test]
    fn the_pen_adds_and_removes_corners_and_knows_when_the_loop_can_close() {
        let mut outline = Outline::new();
        assert!(outline.is_empty() && outline.first().is_none() && outline.bounds().is_none());
        assert!(outline.add([0, 0, 0]));
        assert!(!outline.add([0, 0, 0]), "the corner just placed is not placed twice");
        assert!(outline.add([4, 0, 0]));
        assert!(!outline.would_close([0, 0, 0]), "two corners bound nothing");
        assert!(outline.add([4, 2, 0]) && outline.add([0, 2, 0]));
        assert_eq!(outline.len(), 4);
        assert!(outline.would_close([0, 0, 0]));
        assert!(!outline.would_close([4, 0, 0]), "only the first corner closes the loop");
        assert_eq!((outline.first(), outline.last()), (Some([0, 0, 0]), Some([0, 2, 0])));

        assert_eq!(outline.index_of([4, 2, 0]), Some(2));
        assert_eq!(outline.index_of([9, 9, 9]), None);
        assert!(outline.remove_at(2) && !outline.remove_at(3));
        assert_eq!(outline.points(), [[0, 0, 0], [4, 0, 0], [0, 2, 0]], "the gap heals");
        assert_eq!(outline.bounds(), Some([[0, 0, 0], [4, 2, 0]]));

        outline.curved = true;
        outline.clear();
        assert!(outline.is_empty() && outline.curved, "the corners go, the settings stay");
    }

    #[test]
    fn an_outline_holds_no_more_corners_than_its_limit() {
        let mut outline = Outline::new();
        for i in 0..MAX_POINTS as i32 {
            assert!(!outline.is_full());
            assert!(outline.add([i, i % 2, 0]));
        }
        assert!(outline.is_full());
        assert!(!outline.add([99, 0, 0]));
        assert_eq!(outline.len(), MAX_POINTS);
        let received = Outline::from_points((0..200).map(|i| [i, i % 2, 0]));
        assert_eq!(received.len(), MAX_POINTS, "what arrives from outside is cut to the limit");
        assert_eq!(Outline::new().curve_radius, DEFAULT_CURVE_RADIUS);
    }

    #[test]
    fn a_straight_wall_unrolls_to_its_own_rectangle() {
        let flat = surface(&wall(4, 2, 7));
        assert_eq!((flat.width(), flat.height()), (4.0, 2.0));
        assert_eq!((flat.min_u(), flat.max_u(), flat.min_v(), flat.max_v()), (0.0, 4.0, 0.0, 2.0));
        assert_eq!(flat.size_in_blocks(), [4, 2]);
        assert_eq!(spine_xz(&flat), [[0.0, 7.0], [4.0, 7.0]]);
        assert_eq!(flat.outline_uv(), [[0.0, 0.0], [4.0, 0.0], [4.0, 2.0], [0.0, 2.0]]);
        assert_eq!(flat.block_bounds(), [[0, 0, 7], [4, 2, 7]]);

        assert_eq!(flat.spans_at(1.5).as_slice(), [[0.0, 2.0]]);
        assert!(flat.contains(1.5, 2.0) && !flat.contains(1.5, 2.1));
        assert_eq!(flat.to_world(1.5, 0.5), [1.5, 0.5, 7.0]);
        assert_eq!(flat.to_world(-3.0, 0.5), [0.0, 0.5, 7.0], "before the spine is its first end");
        assert_eq!(flat.to_world(9.0, 0.5), [4.0, 0.5, 7.0], "past it, its last");
        assert_eq!(flat.normal_at(1.5), [0.0, 1.0], "the spine runs along +X, its normal along +Z");
        assert_eq!(flat.offset_at(1.5, 0.5), [0.0, 0.5]);
        assert_eq!(flat.offset_at(0.0, 0.5), [0.0, 0.5], "an end of the spine has one normal only");
        assert_eq!(flat.offset_at(4.0, -0.5), [0.0, -0.5]);
    }

    #[test]
    fn a_right_angle_corner_unrolls_to_its_two_walls_end_to_end() {
        let flat = surface(&sheet(&L, 0, 2));
        assert_eq!(spine_xz(&flat), [[0.0, 0.0], [3.0, 0.0], [3.0, 3.0]]);
        assert_eq!((flat.width(), flat.height()), (6.0, 2.0));
        assert_eq!(flat.size_in_blocks(), [6, 2]);
        assert_eq!(flat.to_world(2.0, 1.0), [2.0, 1.0, 0.0]);
        assert_eq!(flat.to_world(4.5, 1.0), [3.0, 1.0, 1.5], "past the corner the sheet runs along +Z");
        assert_eq!(flat.normal_at(2.0), [0.0, 1.0]);
        assert_eq!(flat.normal_at(4.5), [-1.0, 0.0]);
        assert_eq!(flat.normal_at(3.0), [0.0, 1.0], "the corner itself goes with the wall arriving");
        assert_eq!(flat.spans_at(3.0).as_slice(), [[0.0, 2.0]]);
    }

    #[test]
    fn rounding_cuts_the_corner_and_shortens_the_sheet() {
        let flat = surface(&rounded(sheet(&L, 0, 2), 1.5));
        // Two ends, and the corner replaced by seven points of curve.
        assert_eq!(flat.spine().len(), 9);
        let xz = spine_xz(&flat);
        assert_eq!((xz[0], xz[8]), ([0.0, 0.0], [3.0, 3.0]), "the ends stay on their corners");
        assert!(close2(xz[1], [1.5, 0.0]) && close2(xz[7], [3.0, 1.5]), "the curve takes 1.5 of each wall");
        assert!(close2(xz[4], [2.625, 0.375]), "its middle is inside the corner: {:?}", xz[4]);
        assert!(flat.width() > 5.3 && flat.width() < 5.5, "less than the sharp corner's 6: {}", flat.width());
        assert_eq!(flat.size_in_blocks(), [5, 2]);

        // The traced corner is on no node any more: it takes the place of the nearest point.
        let corner = flat.outline_uv()[1];
        assert!(close(corner[0], flat.spine()[4].u), "{corner:?}");
        let turning = flat.normal_at(flat.spine()[4].u + 0.01);
        assert!(turning[0] < -0.5 && turning[1] > 0.5, "half way round the bend: {turning:?}");
    }

    #[test]
    fn rounding_needs_a_radius_and_stops_where_the_walls_leave_no_room() {
        let sharp = surface(&sheet(&L, 0, 2));
        for nothing in [0.0, -1.0, f32::NAN] {
            let flat = surface(&rounded(sheet(&L, 0, 2), nothing));
            assert_eq!(spine_xz(&flat), spine_xz(&sharp), "radius {nothing}");
        }
        let mut not_curved = sheet(&L, 0, 2);
        not_curved.curve_radius = 1.0;
        assert_eq!(surface(&not_curved), sharp, "a radius alone rounds nothing");

        // Two walls of 3 with one bend leave room for 3 at most, whatever is asked for.
        let most = spine_xz(&surface(&rounded(sheet(&L, 0, 2), 3.0)));
        assert_ne!(spine_xz(&surface(&rounded(sheet(&L, 0, 2), 1.5))), most);
        assert_eq!(spine_xz(&surface(&rounded(sheet(&L, 0, 2), 8.0))), most);
        assert_eq!(spine_xz(&surface(&rounded(sheet(&L, 0, 2), f32::INFINITY))), most);
        let gentle = surface(&rounded(sheet(&L, 0, 2), 0.5));
        assert!(close2(spine_xz(&gentle)[1], [2.5, 0.0]));
        assert!(gentle.width() > surface(&rounded(sheet(&L, 0, 2), 1.5)).width());
    }

    #[test]
    fn two_rounded_corners_sharing_a_short_wall_meet_without_a_kink() {
        // With a radius of 1.5 both curves end on the middle of the middle wall, (1.5, 3).
        let flat = surface(&rounded(sheet(&Z, 0, 2), 1.5));
        let xz = spine_xz(&flat);
        let middle: Vec<usize> = (0..xz.len()).filter(|&i| close2(xz[i], [1.5, 3.0])).collect();
        assert_eq!(middle.len(), 1, "the shared point is one node: {xz:?}");
        for pair in flat.spine().windows(2) {
            assert!(pair[1].u - pair[0].u > 1e-3, "no segment without length: {pair:?}");
        }
        // The wall runs along -X there, so its normal is -Z: the face stays on that side, a
        // little turned as the two curves are where they meet.
        let face = flat.offset_at(flat.spine()[middle[0]].u, 0.5);
        assert!(face[1] < -0.45 && face[0].abs() < 0.1, "{face:?}");
    }

    #[test]
    fn the_rounded_spine_stays_within_its_budget() {
        // A staircase of steps 2 long.
        let stairs = |corners: usize| -> Vec<[i32; 2]> {
            (0..corners as i32 + 2).map(|i| [(i + 1) / 2 * 2, i / 2 * 2]).collect()
        };
        let nodes = |outline: Outline| surface(&rounded(outline, 0.5)).spine().len();
        assert_eq!(nodes(sheet(&stairs(10), 0, 2)), 2 + 10 * 7, "room for all six samples a corner");
        assert_eq!(nodes(sheet(&stairs(30), 0, 2)), 2 + 30 * 5, "four when there are thirty corners");

        // Every one of the 64 corners at its own position: 62 bends.
        let corners = stairs(62).into_iter().enumerate().map(|(i, [x, z])| [x, (i % 2) as i32, z]);
        let crowded = Outline::from_points(corners);
        assert_eq!(crowded.len(), MAX_POINTS);
        let most = nodes(crowded);
        assert_eq!(most, 2 + 62 * 3, "never fewer than two");
        assert!(most <= MAX_SPINE_NODES);
    }

    #[test]
    fn a_u_unrolls_along_its_three_walls() {
        let flat = surface(&sheet(&U, 0, 3));
        assert_eq!(spine_xz(&flat), [[0.0, 0.0], [0.0, 2.0], [4.0, 2.0], [4.0, 0.0]]);
        assert_eq!((flat.width(), flat.height()), (8.0, 3.0));
        assert_eq!(flat.to_world(1.0, 1.0), [0.0, 1.0, 1.0]);
        assert_eq!(flat.to_world(4.0, 1.0), [2.0, 1.0, 2.0]);
        assert_eq!(flat.to_world(7.0, 1.0), [4.0, 1.0, 1.0]);
        for u in [0.5, 2.0, 4.0, 6.0, 7.5] {
            assert_eq!(flat.spans_at(u).as_slice(), [[0.0, 3.0]], "at {u}");
        }
        // One face looks out of the U all the way round.
        let normals = [flat.normal_at(1.0), flat.normal_at(4.0), flat.normal_at(7.0)];
        assert_eq!(normals, [[-1.0, 0.0], [0.0, 1.0], [1.0, 0.0]]);
    }

    #[test]
    fn a_u_with_legs_longer_than_its_mouth_is_chained_across_the_mouth() {
        // The nearest position to the first end is across the mouth, not down the leg: the
        // chain takes it, and the sheet is not the U that was traced.
        let flat = surface(&sheet(&[[0, 0], [0, 5], [3, 5], [3, 0]], 0, 2));
        assert_eq!(spine_xz(&flat), [[0.0, 0.0], [3.0, 0.0], [3.0, 5.0], [0.0, 5.0]]);
        assert_eq!(flat.width(), 11.0, "3 + 5 + 3, where the traced U is 5 + 3 + 5");
    }

    #[test]
    fn the_spine_ignores_the_order_the_corners_were_traced_in() {
        let traced = sheet(&[[0, 0], [4, 0], [4, 3], [9, 3]], 0, 2);
        let reference = surface(&traced);
        assert_eq!(spine_xz(&reference), [[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [9.0, 3.0]]);
        assert_eq!(reference.width(), 12.0);
        let backwards: Vec<[f32; 2]> = spine_xz(&reference).into_iter().rev().collect();

        // Every rotation of the loop, both ways round, and a few shuffles of its corners.
        let mut orders: Vec<Vec<Pos>> = Vec::new();
        for start in 0..traced.len() {
            let mut turned = traced.points().to_vec();
            turned.rotate_left(start);
            orders.push(turned.clone());
            turned.reverse();
            orders.push(turned);
        }
        let mut seed = 12345u32;
        for _ in 0..20 {
            let mut shuffled = traced.points().to_vec();
            for i in (1..shuffled.len()).rev() {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                shuffled.swap(i, (seed >> 8) as usize % (i + 1));
            }
            orders.push(shuffled);
        }
        for order in orders {
            let spine = chain_spine(&order).expect("the positions are spread out");
            let spine: Vec<[f32; 2]> = spine.iter().map(|node| [node[0] as f32, node[1] as f32]).collect();
            assert!(spine == spine_xz(&reference) || spine == backwards, "{order:?} gave {spine:?}");
        }

        // The same loop started elsewhere and walked the other way is the same sheet, seen
        // from the other end of its spine at most.
        let mut turned = traced.points().to_vec();
        turned.rotate_left(3);
        turned.reverse();
        let other = surface(&Outline::from_points(turned));
        assert_eq!((other.width(), other.height()), (12.0, 2.0));
        let mirrored = spine_xz(&other) == backwards;
        for u in [0.5, 3.0, 5.5, 8.0, 11.5] {
            let there = if mirrored { 12.0 - u } else { u };
            assert_eq!(other.spans_at(there), reference.spans_at(u), "at {u}");
            assert_eq!(other.to_world(there, 1.0), reference.to_world(u, 1.0), "at {u}");
        }
    }

    #[test]
    fn an_outline_that_bounds_nothing_has_no_surface() {
        assert!(Surface::of(&Outline::new()).is_none());
        assert!(Surface::of(&drawn(&[[0, 0], [4, 2]])).is_none(), "two corners");
        let pole = Outline::from_points([[1, 0, 1], [1, 3, 1], [1, 5, 1]]);
        assert!(Surface::of(&pole).is_none(), "every corner on one vertical line");
        let floor = Outline::from_points([[0, 4, 0], [3, 4, 0], [3, 4, 3]]);
        assert!(Surface::of(&floor).is_none(), "every corner at one height");
        assert!(Surface::of(&drawn(&[[0, 0], [4, 0], [4, 2]])).is_some(), "a triangle is enough");

        let world = World::default();
        for nothing in [Outline::new(), pole, floor] {
            assert_eq!(plan_build(&nothing, &world), Err(Refusal::Degenerate));
        }
    }

    #[test]
    fn a_point_of_the_sheet_goes_to_the_world_and_back() {
        let shapes = [
            wall(4, 2, 7),
            sheet(&L, 0, 2),
            rounded(sheet(&L, 0, 2), 1.5),
            sheet(&U, 0, 2),
            rounded(sheet(&U, 0, 2), 1.0),
            rounded(sheet(&Z, 0, 2), 1.5),
            sheet(&[[0, 0], [5, 2], [7, 7]], 0, 2),
            rounded(sheet(&[[-3, -8], [2, -6], [4, -1], [9, 0]], 0, 2), 1.5),
        ];
        for outline in &shapes {
            let flat = surface(outline);
            // Up to the far end, not onto it: an edge of the outline belongs to the side
            // further along, and there is none there.
            for step in 0..=20 {
                let u = (flat.width() * step as f32 / 20.0).min(flat.width() - 1e-3);
                for v in [0.0, 0.3, 1.0, 1.99, 2.0] {
                    let [x, y, z] = flat.to_world(u, v);
                    let back = flat.from_world(x as f64, y as f64, z as f64);
                    let back = back.unwrap_or_else(|| panic!("({u}, {v}) is on the sheet of {outline:?}"));
                    assert!(close2(back, [u, v]), "({u}, {v}) came back as {back:?} on {outline:?}");
                }
            }
        }
    }

    #[test]
    fn a_hit_beside_the_sheet_lands_on_its_nearest_point_or_nowhere() {
        let flat = surface(&sheet(&L, 0, 2));
        // The block carrying the sheet is hit on its far face: the height and the place
        // along the spine still tell the point.
        assert_eq!(flat.from_world(1.25, 0.5, 1.0), Some([1.25, 0.5]));
        assert_eq!(flat.from_world(2.0, 0.5, 2.5), Some([5.5, 0.5]), "nearer the second wall");
        assert_eq!(flat.from_world(-4.0, 1.0, 0.0), Some([0.0, 1.0]), "past the end is the end");
        assert_eq!(flat.from_world(1.25, 2.5, 0.0), None, "above the sheet");
        assert_eq!(flat.from_world(1.25, -0.5, 0.0), None, "below it");
        assert_eq!(flat.from_world(f64::NAN, 1.0, 0.0), Some([0.0, 1.0]));
        assert_eq!(flat.from_world(1.0, f64::NAN, 0.0), None);
        assert_eq!(flat.nearest_u(3.5, 1.0), 4.0);
    }

    #[test]
    fn a_point_of_the_sheet_is_a_pixel_of_the_page_mirrored_on_a_flipped_screen() {
        let mut outline = wall(4, 2, 0);
        let front = surface(&outline);
        assert!(!front.flipped() && front.facing() == 1.0 && outline.facing() == 1.0);
        assert_eq!(front.page_uv(1.0, 1.5), [0.25, 0.25], "a quarter along, a quarter down from the top");
        assert_eq!(front.to_pixels(1.0, 1.5, [400, 200]), [100, 50]);
        assert_eq!(front.to_pixels(0.0, 2.0, [400, 200]), [0, 0], "the top of the start is the first pixel");
        assert_eq!(front.to_pixels(4.0, 0.0, [400, 200]), [399, 199], "the far corner stays on the page");
        assert_eq!(front.to_pixels(9.0, -3.0, [400, 200]), [399, 199]);
        assert_eq!(front.to_pixels(-9.0, 30.0, [400, 200]), [0, 0]);
        assert_eq!(front.to_pixels(1.0, 1.5, [0, 0]), [0, 0], "a page without pixels has none to miss");

        outline.flipped = true;
        let back = surface(&outline);
        assert!(back.flipped() && back.facing() == -1.0 && outline.facing() == -1.0);
        assert_eq!(back.page_uv(1.0, 1.5), [0.75, 0.25]);
        assert_eq!(back.to_pixels(1.0, 1.5, [400, 200]), [300, 50], "the other end of the page, as high");
        assert_eq!(back.to_pixels(0.0, 2.0, [400, 200]), [399, 0]);
        assert_eq!(back.panel_uv(1.0, 1.5), front.panel_uv(1.0, 1.5), "the panel behind is not mirrored");
        // Flipping moves nothing: the same sheet, the same cells.
        assert_eq!(back.to_world(1.0, 1.5), front.to_world(1.0, 1.5));
        assert_eq!(back.cell_candidates(), front.cell_candidates());
    }

    #[test]
    fn a_notch_cut_from_the_top_keeps_the_width_and_lowers_the_sheet_there() {
        let flat = surface(&drawn(&NOTCHED));
        assert_eq!(spine_xz(&flat), [[0.0, 0.0], [3.0, 0.0], [7.0, 0.0], [10.0, 0.0]]);
        assert_eq!(flat.width(), 10.0, "the way back over the notch adds nothing");
        assert_eq!(flat.outline_uv()[3..7], [[7.0, 4.0], [7.0, 2.0], [3.0, 2.0], [3.0, 4.0]]);
        assert_eq!(flat.spans_at(1.0).as_slice(), [[0.0, 4.0]]);
        assert_eq!(flat.spans_at(5.0).as_slice(), [[0.0, 2.0]]);
        assert_eq!(flat.spans_at(9.0).as_slice(), [[0.0, 4.0]]);
        assert!(flat.contains(5.0, 1.0) && !flat.contains(5.0, 3.0));
        assert_eq!(flat.from_world(5.0, 3.0, 0.0), None, "a click in the notch is nobody's");
        // A corner counts for one of its two edges only: exactly on a wall of the notch the
        // sheet is that of the side further along.
        assert_eq!(flat.spans_at(3.0).as_slice(), [[0.0, 2.0]]);
        assert_eq!(flat.spans_at(7.0).as_slice(), [[0.0, 4.0]]);
        assert!(flat.spans_at(-0.5).is_empty() && flat.spans_at(10.0).is_empty());
        assert!(flat.spans_at(f32::NAN).is_empty());
    }

    #[test]
    fn an_arch_and_a_slot_leave_the_sheet_in_pieces_one_above_the_other() {
        // An arch: the notch is cut from the bottom.
        let arch = surface(&drawn(&[[0, 0], [3, 0], [3, 2], [7, 2], [7, 0], [10, 0], [10, 4], [0, 4]]));
        assert_eq!(arch.spans_at(5.0).as_slice(), [[2.0, 4.0]]);
        assert!(!arch.contains(5.0, 1.0) && arch.contains(5.0, 3.0));

        // A slot: two runs where it is, one before it.
        let slot = surface(&drawn(&SLOTTED));
        assert_eq!(slot.spans_at(1.0).as_slice(), [[0.0, 4.0]]);
        assert_eq!(slot.spans_at(4.0).as_slice(), [[0.0, 1.0], [3.0, 4.0]]);
        assert!(slot.contains(4.0, 0.5) && !slot.contains(4.0, 2.0) && slot.contains(4.0, 3.5));

        // A sloped edge is followed, not stepped.
        let ramp = surface(&drawn(&[[0, 0], [4, 0], [4, 4], [0, 2]]));
        assert_eq!(ramp.spans_at(1.0).as_slice(), [[0.0, 2.5]]);
        assert_eq!(ramp.spans_at(3.0).as_slice(), [[0.0, 3.5]]);
    }

    #[test]
    fn where_the_outline_folds_back_inside_itself_the_even_odd_rule_leaves_a_hole() {
        // Round the outside, in along a cut to an inner rectangle, round it, and back out
        // along the same cut.
        let folded = drawn(&[[0, 4], [0, 0], [2, 1], [4, 1], [4, 3], [2, 3], [2, 1], [0, 0], [6, 0], [6, 4]]);
        let flat = surface(&folded);
        assert_eq!((flat.width(), flat.height()), (6.0, 4.0));
        assert_eq!(flat.spans_at(3.0).as_slice(), [[0.0, 1.0], [3.0, 4.0]], "the inner rectangle is a hole");
        assert!(!flat.contains(3.0, 2.0) && flat.contains(3.0, 0.5) && flat.contains(3.0, 3.5));
        assert_eq!(flat.spans_at(5.0).as_slice(), [[0.0, 4.0]]);
        // The cut is crossed twice, there and back: it takes nothing out of the sheet.
        assert_eq!(flat.spans_at(1.0).as_slice(), [[0.0, 0.5], [0.5, 4.0]]);
        assert!(flat.contains(1.0, 0.5) && flat.contains(1.0, 2.0));
        assert_eq!(flat.from_world(3.0, 2.0, 0.0), None);

        let plan = plan_build(&folded, &World::default()).unwrap();
        assert!(!plan.cells.contains(&[3, 1, 0]) && !plan.cells.contains(&[3, 2, 0]), "no block in the hole");
        assert!(plan.cells.contains(&[3, 0, 0]) && plan.cells.contains(&[3, 3, 0]));
        assert!(plan.cells.contains(&[1, 2, 0]));
        assert_eq!(plan.cells.len(), 6 * 4 - 2 * 2);
    }

    #[test]
    fn the_thickness_of_the_sheet_is_mitred_at_a_bend() {
        let flat = surface(&sheet(&L, 0, 2));
        assert_eq!(flat.offset_at(1.0, 0.5), [0.0, 0.5]);
        assert_eq!(flat.offset_at(5.0, 0.5), [-0.5, 0.0]);
        // At the corner both faces meet on the diagonal, as far from each wall as elsewhere.
        assert!(close2(flat.offset_at(3.0, 0.5), [-0.5, 0.5]), "{:?}", flat.offset_at(3.0, 0.5));
        assert!(close2(flat.offset_at(3.00005, 0.5), [-0.5, 0.5]), "a seam this close to a node is on it");
        assert!(close2(flat.offset_at(3.0, -0.5), [0.5, -0.5]));
        assert_eq!(flat.offset_at(3.001, 0.5), [-0.5, 0.0], "past the corner, the second wall's own normal");
        assert_eq!(flat.offset_at(6.0, 0.5), [-0.5, 0.0]);
        assert_eq!(flat.offset_at(60.0, 0.5), [-0.5, 0.0]);

        // A fold close to a half turn would send the mitre far away: it stops at four times
        // the distance. No traced outline chains into one, so the spine is made by hand.
        let node = |x: f32, z: f32, u: f32| SpineNode { x, z, u };
        let spine = vec![node(0.0, 0.0, 0.0), node(10.0, 0.0, 10.0), node(0.0, 0.2, 20.0)];
        let fold = Surface { spine, ..flat };
        let tip = fold.offset_at(10.0, 0.5);
        assert!(close(distance([0.0, 0.0], tip), 2.0), "{tip:?}");
    }

    /// The seams as a plain list: every candidate sorted, then those within the tolerance of
    /// the one before dropped.
    fn seams_the_long_way(flat: &Surface) -> Vec<f32> {
        let mut all = vec![flat.min_u(), flat.max_u()];
        all.extend(flat.spine().iter().map(|node| node.u));
        all.extend(flat.outline_uv().iter().map(|corner| corner[0]));
        let mut block = flat.min_u().ceil();
        while block < flat.max_u() {
            all.push(block);
            block += 1.0;
        }
        all.sort_by(f32::total_cmp);
        let mut seams: Vec<f32> = Vec::new();
        for u in all {
            if seams.last().is_none_or(|last| (u - last).abs() >= 1.0e-4) {
                seams.push(u);
            }
        }
        seams
    }

    #[test]
    fn seams_fall_on_spine_nodes_outline_corners_and_whole_blocks() {
        let straight = surface(&wall(4, 2, 0));
        assert_eq!(straight.breakpoints().collect::<Vec<_>>(), [0.0, 1.0, 2.0, 3.0, 4.0]);

        // A slanted wall: its length is no whole number, its far end is still a seam.
        let slanted = surface(&sheet(&[[0, 0], [3, 1]], 0, 2));
        let seams: Vec<f32> = slanted.breakpoints().collect();
        assert_eq!(seams[..4], [0.0, 1.0, 2.0, 3.0]);
        assert!(seams.len() == 5 && close(seams[4], 10f32.sqrt()), "{seams:?}");

        // A corner of the outline between two whole blocks.
        let peak = surface(&Outline::from_points([[0, 0, 0], [4, 0, 0], [4, 2, 0], [1, 3, 1], [0, 2, 0]]));
        let seams: Vec<f32> = peak.breakpoints().collect();
        assert!(seams.windows(2).all(|pair| pair[1] - pair[0] >= 1.0e-4), "in order, none twice: {seams:?}");
        for node in peak.spine() {
            assert!(seams.iter().any(|&seam| close(seam, node.u)), "node at {} in {seams:?}", node.u);
        }
        for corner in peak.outline_uv() {
            assert!(seams.iter().any(|&seam| close(seam, corner[0])), "corner at {} in {seams:?}", corner[0]);
        }

        let shapes = [
            wall(4, 2, 0),
            sheet(&L, 0, 2),
            rounded(sheet(&L, 0, 2), 1.5),
            rounded(sheet(&Z, 0, 2), 1.5),
            sheet(&[[0, 0], [5, 2], [7, 7]], 0, 2),
            drawn(&NOTCHED),
            Outline::from_points([[0, 0, 0], [4, 0, 0], [4, 2, 0], [1, 3, 1], [0, 2, 0]]),
        ];
        for outline in &shapes {
            let flat = surface(outline);
            assert_eq!(flat.breakpoints().collect::<Vec<_>>(), seams_the_long_way(&flat), "{outline:?}");
        }
    }

    #[test]
    fn strips_follow_a_sloped_edge_and_know_the_ends_of_the_sheet() {
        // Flat for two blocks, then rising from 2 to 4 over the next two.
        let ramp = surface(&drawn(&[[0, 0], [4, 0], [4, 4], [2, 2], [0, 2]]));
        let strips: Vec<Strip> = ramp.strips().collect();
        let seams: Vec<[f32; 2]> = strips.iter().map(|strip| strip.u).collect();
        assert_eq!(seams, [[0.0, 1.0], [1.0, 2.0], [2.0, 3.0], [3.0, 4.0]]);
        let caps: Vec<[bool; 2]> = strips.iter().map(|strip| strip.caps).collect();
        assert_eq!(caps, [[true, false], [false, false], [false, false], [false, true]]);
        assert!(close2(strips[1].left, [0.0, 2.0]) && close2(strips[1].right, [0.0, 2.0]));
        assert!(close2(strips[2].left, [0.0, 2.0]) && close2(strips[2].right, [0.0, 3.0]), "{:?}", strips[2]);
        assert!(close2(strips[3].left, [0.0, 3.0]) && close2(strips[3].right, [0.0, 4.0]));

        // Two runs at one place give two strips between the same seams, the lower first.
        let slot = surface(&drawn(&SLOTTED));
        let strips: Vec<Strip> = slot.strips().collect();
        assert_eq!(strips.len(), 2 + 4 * 2);
        assert_eq!((strips[2].u, strips[3].u), ([2.0, 3.0], [2.0, 3.0]));
        assert!(close2(strips[2].left, [0.0, 1.0]) && close2(strips[3].left, [3.0, 4.0]));

        // Round a bend no strip crosses a spine node, and together they cover the sheet.
        let bent = surface(&rounded(sheet(&L, 0, 2), 1.5));
        let strips: Vec<Strip> = bent.strips().collect();
        let covered: f32 = strips.iter().map(|strip| strip.u[1] - strip.u[0]).sum();
        assert!(close(covered, bent.width()));
        for strip in &strips {
            let inside = |node: &&SpineNode| node.u > strip.u[0] + 1e-3 && node.u < strip.u[1] - 1e-3;
            assert_eq!(bent.spine().iter().filter(inside).count(), 0, "{strip:?}");
            assert_eq!((strip.left, strip.right), ([0.0, 2.0], [0.0, 2.0]));
        }
        assert_eq!(strips.iter().filter(|strip| strip.caps[0]).count(), 1);
        assert_eq!(strips.iter().filter(|strip| strip.caps[1]).count(), 1);
    }

    #[test]
    fn each_column_offers_the_two_cells_either_side_of_the_sheet() {
        let flat = surface(&wall(3, 2, 5));
        // The sheet is on the face between z = 4 and z = 5; the cell its normal points into
        // comes first. One pair for each column at each height, from the start of the spine.
        let columns = (0..3).flat_map(|x| (0..2).map(move |y| [[x, y, 5], [x, y, 4]]));
        assert_eq!(flat.cell_candidates(), columns.collect::<Vec<[Pos; 2]>>());

        // Where the sheet is cut, so are its columns.
        let notched = surface(&drawn(&[[0, 0], [3, 0], [3, 3], [2, 3], [2, 1], [1, 1], [1, 3], [0, 3]]));
        let preferred: BTreeSet<Pos> = notched.cell_candidates().iter().map(|pair| pair[0]).collect();
        let expected = [[0, 0, 0], [0, 1, 0], [0, 2, 0], [1, 0, 0], [2, 0, 0], [2, 1, 0], [2, 2, 0]];
        assert_eq!(preferred, BTreeSet::from(expected));
    }

    #[test]
    fn a_bend_lists_no_pair_of_cells_twice() {
        let pairs = surface(&sheet(&L, 0, 1)).cell_candidates();
        // No column past the corner: the first wall ends at x = 3, in the cell x = 2.
        let expected: [[Pos; 2]; 6] = [
            [[0, 0, 0], [0, 0, -1]],
            [[1, 0, 0], [1, 0, -1]],
            [[2, 0, 0], [2, 0, -1]],
            [[2, 0, 0], [3, 0, 0]],
            [[2, 0, 1], [3, 0, 1]],
            [[2, 0, 2], [3, 0, 2]],
        ];
        assert_eq!(pairs, expected);

        // On a slant both sides can fall in one cell: the pair is that cell twice.
        let pairs = surface(&sheet(&[[0, 0], [3, 3]], 0, 1)).cell_candidates();
        assert!(pairs.contains(&[[0, 0, 0], [0, 0, 0]]), "{pairs:?}");
        let either_way = |&[a, b]: &[Pos; 2]| if a <= b { [a, b] } else { [b, a] };
        let mut unordered: Vec<[Pos; 2]> = pairs.iter().map(either_way).collect();
        unordered.sort();
        unordered.dedup();
        assert_eq!(unordered.len(), pairs.len(), "{pairs:?}");
        for cell in [[0, 0, 0], [1, 0, 1], [2, 0, 2]] {
            assert!(pairs.iter().flatten().any(|&offered| offered == cell), "the sheet crosses {cell:?}");
        }
    }

    #[test]
    fn a_free_world_gives_the_preferred_cells_and_the_first_is_the_master() {
        let outline = wall(4, 2, 0);
        let plan = plan_build(&outline, &World::default()).unwrap();
        let expected: Vec<Pos> = (0..4).flat_map(|x| (0..2).map(move |y| [x, y, 0])).collect();
        assert_eq!(plan.cells, expected);
        assert_eq!(plan.master, [0, 0, 0]);
        assert_eq!(plan.size, [4, 2]);
        assert_eq!(plan.surface, surface(&outline));

        // A closure serves as the world too.
        let nothing_above = |pos: Pos| pos[1] < 1;
        assert_eq!(plan_build(&outline, &nothing_above), Err(Refusal::Obstructed));

        // The size given to the screen is the unrolled one, rounded, never nothing.
        let bent = plan_build(&rounded(sheet(&L, 0, 2), 1.5), &World::default()).unwrap();
        assert_eq!(bent.size, [5, 2]);
        let sliver = plan_build(&drawn(&[[0, 0], [1, 0], [1, 1]]), &World::default()).unwrap();
        assert_eq!((sliver.size, sliver.cells), ([1, 1], vec![[0, 0, 0]]));
    }

    #[test]
    fn two_walls_meeting_at_a_bend_take_their_shared_cell_once() {
        let plan = plan_build(&sheet(&L, 0, 1), &World::default()).unwrap();
        assert_eq!(plan.cells, [[0, 0, 0], [1, 0, 0], [2, 0, 0], [2, 0, 1], [2, 0, 2]]);
        assert_eq!(plan.master, [0, 0, 0]);
    }

    #[test]
    fn a_taken_cell_gives_way_to_the_one_across_the_sheet() {
        let outline = wall(4, 2, 0);
        let mut world = World::default();
        world.fill(&[[1, 0, 0], [2, 1, 0]], 0);
        let plan = plan_build(&outline, &world).unwrap();
        let columns = [[[0, 0, 0], [0, 1, 0]], [[1, 0, -1], [1, 1, 0]], [[2, 0, 0], [2, 1, -1]]];
        assert_eq!(plan.cells, [columns.concat(), vec![[3, 0, 0], [3, 1, 0]]].concat());

        // Flush against a wall the whole sheet stands on the open side, master included.
        let mut against_a_wall = World::default();
        against_a_wall.fill(&plan_build(&outline, &World::default()).unwrap().cells, 0);
        let plan = plan_build(&outline, &against_a_wall).unwrap();
        assert!(plan.cells.iter().all(|cell| cell[2] == -1), "{:?}", plan.cells);
        assert_eq!((plan.master, plan.cells.len()), ([0, 0, -1], 8));
    }

    #[test]
    fn a_column_taken_on_both_sides_refuses_the_whole_build() {
        let mut world = World::default();
        world.fill(&[[3, 1, 0], [3, 1, -1]], 0);
        assert_eq!(plan_build(&wall(4, 2, 0), &world), Err(Refusal::Obstructed));
        world.clear(&[[3, 1, -1]]);
        assert!(plan_build(&wall(4, 2, 0), &world).is_ok());
    }

    #[test]
    fn each_limit_refuses_with_its_own_reason() {
        let world = World::default();
        let refused = |outline: Outline| plan_build(&outline, &world).err();

        assert_eq!(refused(wall(64, 16, 0)), None, "64 wide and 1024 cells are allowed");
        assert_eq!(plan_build(&wall(64, 16, 0), &world).unwrap().cells.len(), MAX_CELLS);
        assert_eq!(refused(wall(65, 2, 0)), Some(Refusal::TooWide));
        assert_eq!(refused(wall(32, 32, 0)), None, "32 high is allowed");
        assert_eq!(refused(wall(4, 33, 0)), Some(Refusal::TooTall));
        assert_eq!(refused(wall(64, 17, 0)), Some(Refusal::TooManyCells));
        assert_eq!(refused(wall(65, 33, 0)), Some(Refusal::TooWide), "the width is looked at first");
        assert_eq!(refused(wall(3_000_000, 2, 0)), Some(Refusal::TooWide), "without walking its length");

        // The width is the unrolled one: an L of 40 and 30 is 70 long, wherever its ends are.
        assert_eq!(refused(sheet(&[[0, 0], [40, 0], [40, 30]], 0, 2)), Some(Refusal::TooWide));
        assert_eq!(refused(sheet(&[[0, 0], [40, 0], [40, 24]], 0, 2)), None);

        use Refusal::*;
        let reasons = [Degenerate, TooWide, TooTall, TooManyCells, Obstructed].map(Refusal::reason);
        assert_eq!(reasons.clone().map(|reason| reason.english()), ["degenerate", "too wide", "too tall", "too many cells", "obstructed"]);
        let keys: BTreeSet<&str> = reasons.iter().map(|reason| reason.key).collect();
        assert_eq!(keys.len(), reasons.len(), "each is translated under its own key");
    }

    #[test]
    fn a_shape_with_extent_but_no_cell_to_stand_in_is_degenerate() {
        // Out along the ground and back the same way, with a spike at the far end: it has a
        // width and a height, and no area.
        let flagpole = drawn(&[[0, 0], [4, 0], [4, 1], [4, 0]]);
        let flat = surface(&flagpole);
        assert_eq!((flat.width(), flat.height()), (4.0, 1.0));
        assert!(flat.cell_candidates().is_empty());
        assert_eq!(plan_build(&flagpole, &World::default()), Err(Refusal::Degenerate));
    }

    #[test]
    fn demolishing_one_of_two_sheets_side_by_side_leaves_the_others_cells_alone() {
        let mut world = World::default();
        let first = plan_build(&wall(4, 2, 0), &world).unwrap();
        world.fill(&first.cells, 1);
        let second = plan_build(&wall(4, 2, 1), &world).unwrap();
        world.fill(&second.cells, 2);
        assert!(first.cells.iter().all(|cell| cell[2] == 0) && second.cells.iter().all(|cell| cell[2] == 1));
        // The far side of the second sheet is the first sheet's row of cells.
        let pairs = second.surface.cell_candidates();
        assert_eq!(pairs.iter().filter(|pair| first.cells.contains(&pair[1])).count(), 8);

        let gone = plan_demolish(&second.surface, second.master, None, world.owned_by(2));
        let listed: BTreeSet<Pos> = gone.iter().copied().collect();
        assert_eq!(listed, world.cells_of(2), "all of the second sheet and nothing else");
        assert_eq!(gone.len(), 8, "each cell once");
        assert_eq!(gone.last(), Some(&second.master), "the master goes last");
        world.clear(&gone);
        assert_eq!(world.cells_of(1).len(), 8, "the first sheet stands");
        assert!(world.cells_of(2).is_empty());

        let gone = plan_demolish(&first.surface, first.master, None, world.owned_by(1));
        world.clear(&gone);
        assert!(world.blocks.is_empty());
    }

    #[test]
    fn demolishing_skips_the_cell_the_game_is_already_removing() {
        let mut world = World::default();
        let plan = plan_build(&wall(3, 1, 0), &world).unwrap();
        world.fill(&plan.cells, 1);
        world.fill(&[[1, 0, -1]], 0);

        let gone = plan_demolish(&plan.surface, plan.master, Some([1, 0, 0]), world.owned_by(1));
        assert_eq!(gone, [[2, 0, 0], [0, 0, 0]], "the rest, then the master; the wall behind is not ours");
        let gone = plan_demolish(&plan.surface, plan.master, Some(plan.master), world.owned_by(1));
        assert_eq!(gone, [[1, 0, 0], [2, 0, 0]], "a master being broken is not emptied again");
        // The master is the screen asked for, whatever the owner test makes of it.
        let gone = plan_demolish(&plan.surface, plan.master, None, |_| false);
        assert_eq!(gone, [plan.master]);
    }

    #[test]
    fn refitting_moves_this_screens_cells_only_and_keeps_its_master() {
        let nothing = RefitPlan { remove: vec![], add: vec![] };
        let set = |cells: &[Pos]| -> BTreeSet<Pos> { cells.iter().copied().collect() };
        // Two walls of 5 meeting at (5, 0).
        let mut outline = sheet(&[[0, 0], [5, 0], [5, 5]], 0, 1);
        let mut world = World::default();
        let plan = plan_build(&outline, &world).unwrap();
        world.fill(&plan.cells, 1);
        // A neighbouring screen has cells in the box too.
        world.fill(&[[1, 0, -1], [5, 0, 5]], 2);
        let unchanged = plan_refit(&plan.surface, plan.master, &world, world.owned_by(1));
        assert_eq!(unchanged, nothing, "the shape is the same, so are its cells");

        // Rounded as far as the walls allow, the sheet cuts across the corner: cells in the
        // corner are left, cells across it are taken.
        outline.curved = true;
        outline.curve_radius = 5.0;
        let bent = surface(&outline);
        let refit = plan_refit(&bent, plan.master, &world, world.owned_by(1));
        assert!(!refit.remove.is_empty() && !refit.add.is_empty(), "{refit:?}");
        assert!(refit.remove.contains(&[4, 0, 0]), "the corner cell is left: {refit:?}");
        assert!(refit.remove.iter().all(|cell| plan.cells.contains(cell) && *cell != plan.master), "{refit:?}");
        assert!(refit.add.iter().all(|cell| !plan.cells.contains(cell) && world.is_free(*cell)), "{refit:?}");
        world.clear(&refit.remove);
        world.fill(&refit.add, 1);
        assert_eq!(world.cells_of(2).len(), 2, "the neighbour is untouched");
        assert_eq!(plan_refit(&bent, plan.master, &world, world.owned_by(1)), nothing, "done once, it is done");

        // A master stays where it is even when the sheet has left its cell.
        world.fill(&[[4, 0, 0]], 1);
        assert_eq!(plan_refit(&bent, [4, 0, 0], &world, world.owned_by(1)), nothing);
        world.clear(&[[4, 0, 0]]);

        // Sharp again: what was left is wanted again, and free; what was taken is left.
        let sharp = plan_refit(&plan.surface, plan.master, &world, world.owned_by(1));
        assert_eq!((set(&sharp.add), set(&sharp.remove)), (set(&refit.remove), set(&refit.add)));

        // Taken by the neighbour, with the cell across the sheet taken as well, a column is
        // left out: the neighbour's cell is neither claimed nor emptied.
        world.fill(&[[4, 0, 0], [4, 0, -1]], 2);
        let around = plan_refit(&plan.surface, plan.master, &world, world.owned_by(1));
        assert!(!around.add.contains(&[4, 0, 0]) && !around.add.contains(&[4, 0, -1]) && around.remove == sharp.remove, "{around:?}");
        assert_eq!(world.cells_of(2).len(), 4);
    }

    #[test]
    fn refitting_claims_the_free_side_and_drops_cells_the_sheet_has_left() {
        let mut world = World::default();
        let plan = plan_build(&wall(4, 1, 0), &world).unwrap();
        world.fill(&plan.cells, 1);
        // The screen holds a stray cell in its box, and has lost one of its own to a block
        // that is no screen's.
        world.fill(&[[2, 1, 0]], 1);
        world.fill(&[[1, 0, 0]], 0);
        let refit = plan_refit(&plan.surface, plan.master, &world, world.owned_by(1));
        assert_eq!(refit.remove, [[2, 1, 0]]);
        assert_eq!(refit.add, [[1, 0, -1]], "the other side of the sheet is free");
    }

    /// The columns of cells a sheet names, as (x, z): the preferred ones, then the alternates.
    fn columns(outline: &Outline) -> [BTreeSet<(i32, i32)>; 2] {
        let mut sides = [BTreeSet::new(), BTreeSet::new()];
        for pair in surface(outline).cell_candidates() {
            for (side, cell) in pair.into_iter().enumerate() {
                sides[side].insert((cell[0], cell[2]));
            }
        }
        sides
    }

    #[test]
    fn a_wall_has_no_column_past_its_ends_whichever_way_it_was_traced() {
        let along = |cells: std::ops::Range<i32>, other: i32, x_first: bool| -> BTreeSet<(i32, i32)> { cells.map(|cell| if x_first { (cell, other) } else { (other, cell) }).collect() };
        // Along X at z = 0: the cells either side are z = 0 and z = -1, x from 0 to 4.
        for path in [[[0, 0], [5, 0]], [[5, 0], [0, 0]]] {
            let [preferred, alternate] = columns(&sheet(&path, 0, 3));
            let both: BTreeSet<_> = preferred.union(&alternate).copied().collect();
            let wanted: BTreeSet<_> = along(0..5, 0, true).union(&along(0..5, -1, true)).copied().collect();
            assert_eq!(both, wanted, "{path:?}");
            assert_eq!(preferred.len(), 5, "{path:?}");
        }
        // Along Z at x = 0.
        for path in [[[0, 0], [0, 5]], [[0, 5], [0, 0]]] {
            let [preferred, alternate] = columns(&sheet(&path, 0, 3));
            let both: BTreeSet<_> = preferred.union(&alternate).copied().collect();
            let wanted: BTreeSet<_> = along(0..5, 0, false).union(&along(0..5, -1, false)).copied().collect();
            assert_eq!(both, wanted, "{path:?}");
            assert_eq!(preferred.len(), 5, "{path:?}");
        }
    }

    #[test]
    fn a_sharp_bend_has_no_column_past_its_corner() {
        // Two walls of 5 meeting at (5, 0): inside the bend the cells are x <= 4, z >= 0.
        for path in [[[0, 0], [5, 0], [5, 5]], [[5, 5], [5, 0], [0, 0]]] {
            let [preferred, alternate] = columns(&sheet(&path, 0, 3));
            let inside: BTreeSet<(i32, i32)> = (0..5).map(|x| (x, 0)).chain((0..5).map(|z| (4, z))).collect();
            let outside: BTreeSet<(i32, i32)> = (0..5).map(|x| (x, -1)).chain((0..5).map(|z| (5, z))).collect();
            let sides: BTreeSet<_> = [preferred.clone(), alternate.clone()].into_iter().collect();
            assert_eq!(sides, BTreeSet::from([inside, outside]), "{path:?}");
        }
    }

    #[test]
    fn a_corner_takes_the_whole_of_a_wall_it_does_not_share() {
        // One bend between two walls of 5: nothing else rounds them, so the radius counts up to 5.
        let l = [[0, 0], [5, 0], [5, 5]];
        assert_eq!(sheet(&l, 0, 3).widest_radius(), 5.0);
        let width = |radius: f32| surface(&rounded(sheet(&l, 0, 3), radius)).width();
        assert!(width(2.5) > width(4.0) && width(4.0) > width(5.0), "each radius up to the widest shortens the sheet");
        assert_eq!(width(5.0), width(8.0), "past it nothing changes");
        // A U: its mouth is shared by two corners, half each; its legs are theirs alone.
        assert_eq!(sheet(&U, 0, 3).widest_radius(), 2.0);
        let z_width = |radius: f32| surface(&rounded(sheet(&Z, 0, 3), radius)).width();
        assert_eq!(sheet(&Z, 0, 3).widest_radius(), 1.5, "the middle wall of 3 is shared");
        assert_eq!(z_width(1.5), z_width(3.0));
        // A straight wall has no bend to round.
        assert_eq!(wall(6, 3, 0).widest_radius(), 0.0);
    }

    #[test]
    fn a_screen_is_removed_with_the_cells_an_older_sheet_left_it() {
        let outline = sheet(&[[5, 0], [0, 0]], 0, 3);
        let surface = surface(&outline);
        let master = [4, 0, -1];
        // What a version walking on whole steps built: a column past the wall's first corner.
        let stray = [[5, 0, -1], [5, 1, -1], [5, 2, -1]];
        let named: HashSet<Pos> = surface.cell_candidates().into_iter().map(|pair| pair[0]).collect();
        assert!(stray.iter().all(|cell| !named.contains(cell)));
        let owned = |cell: Pos| named.contains(&cell) || stray.contains(&cell);
        let gone = plan_demolish(&surface, master, None, owned);
        assert_eq!(gone.len(), named.len() + stray.len());
        assert!(stray.iter().all(|cell| gone.contains(cell)));
        assert_eq!(gone.last(), Some(&master), "the master goes last");
        assert_eq!(gone.iter().collect::<HashSet<_>>().len(), gone.len(), "each cell once");
    }

    #[test]
    fn a_rounded_bend_has_no_column_past_the_ends_of_its_walls() {
        // Two walls of 4 from (-34, 13) by (-34, 9) to (-30, 9): whatever the radius and
        // whichever way they were traced, the cells either side stay beside the walls, x from
        // -35 to -31 and z from 8 to 12. Rounded all the way (4), the sheet leaves both ends a
        // little askew, and a look from the very end stepped into (-34, 13) and (-30, 9).
        for path in [[[-34, 13], [-34, 9], [-30, 9]], [[-30, 9], [-34, 9], [-34, 13]]] {
            for radius in [0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 8.0] {
                let flat = surface(&rounded(sheet(&path, 80, 84), radius));
                let pairs = flat.cell_candidates();
                assert!(!pairs.is_empty());
                for cell in pairs.iter().flatten() {
                    assert!((-35..=-31).contains(&cell[0]) && (8..=12).contains(&cell[2]) && (80..84).contains(&cell[1]), "radius {radius}, {path:?}: {cell:?}");
                }
                // Every block of wall away from the bend still has its column.
                for along in [10, 11, 12] {
                    assert!(pairs.iter().flatten().any(|cell| cell[0] == -34 && cell[2] == along) || radius > 2.0, "radius {radius}: z = {along}");
                }
                assert!(pairs.iter().flatten().any(|cell| cell[2] == 12) && pairs.iter().flatten().any(|cell| cell[0] == -31), "radius {radius}: both ends are carried");
            }
        }
    }
}
