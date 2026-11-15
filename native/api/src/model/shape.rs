//! The shape of a screen: which blocks it is made of, and where its edge runs.
//!
//! A screen is the screen blocks that touch each other, corners included, in one plane and
//! seen on one face of the blocks. The plane is measured in cells: `x` along the face's
//! `right`, `y` along its `up`, cell (0, 0) at the screen's origin, the bottom left corner
//! of the rectangle around it. The origin is a corner of that rectangle and need not be a
//! screen block itself (the foot of a T is not at its corner).
//!
//! Two things are found here, and the Java mod found them the same way:
//!
//! - `Component`: the blocks joined to a given one. It is what tells where a screen starts
//!   and how large it is when a player turns it on, adds a block to it or clicks it.
//! - `Shape`: what a screen of a given origin and size covers, and what its shape mode adds
//!   to the edge. The renderer, the click path and the ambient light all read this one
//!   value, so that a click lands on what is drawn.
//!
//! Surprises worth knowing:
//!
//! - The search for a component stops a maximum screen away from where it started, each way.
//!   It can therefore come back wider than the maximum (the caller refuses it: `fits`), and
//!   one that goes on further is `is_cut_off`: its origin then depends on the starting block.
//! - A screen's saved size only ever grows, so its rectangle can hold more than its blocks.
//!   A `Shape` keeps the blocks joined to its seed inside the rectangle and nothing else.
//! - A column's or a row's span runs from its first block to its last and covers the holes
//!   between them; `has_block` is what tells a hole.
//! - `ShapeMode::SmoothLong` is kept as the Java mod drew it, oddities included. Next to a
//!   higher neighbour a column's end is lifted to that neighbour's top, however high (so a
//!   staircase of one-block steps looks as in `SmoothOne`). Its other end follows one
//!   straight line drawn over the whole run of columns stepping down, where that line passes
//!   above it. The two do not always meet, so a long flat step shows a notch, and a gap one
//!   block wide between two higher columns is closed. Rows are treated the same way.

use std::collections::HashSet;

use crate::model::side::{Pos, ShapeMode, Side, offset};

/// The world, as far as a shape needs it.
pub trait Blocks {
    /// Whether the block at `pos` is a screen block.
    fn is_screen(&self, pos: Pos) -> bool;
}

impl<F: Fn(Pos) -> bool> Blocks for F {
    fn is_screen(&self, pos: Pos) -> bool {
        self(pos)
    }
}

/// The eight cells around one, in the order the search visits them: `right` first. The
/// order decides which of two blocks at the same distance `nearest_connected` returns.
const NEIGHBOURS: [[i32; 2]; 8] = [[1, 0], [-1, 0], [0, 1], [0, -1], [1, 1], [-1, 1], [1, -1], [-1, -1]];

/// The block at `cell` of the plane of `side` whose cell (0, 0) is `origin`.
pub fn block_at(origin: Pos, side: Side, cell: [i32; 2]) -> Pos {
    offset(offset(origin, side.right(), cell[0]), side.up(), cell[1])
}

/// The cell `pos` falls on in the plane of `side` whose cell (0, 0) is `origin`. A block in
/// front of or behind the plane gets the cell it is in front of.
pub fn cell_of(origin: Pos, side: Side, pos: Pos) -> [i32; 2] {
    let along = |way: Pos| (0..3).map(|i| (pos[i] - origin[i]) * way[i]).sum();
    [along(side.right()), along(side.up())]
}

/// Every block of the rectangle of `size` cells at `origin`, row by row from the bottom,
/// each row from the left: the block of cell (x, y) comes at place `y * width + x`. Holes
/// are not skipped: this is the grid a screen keeps one redstone level per cell of.
pub fn rect_blocks(origin: Pos, side: Side, size: [u32; 2]) -> impl Iterator<Item = Pos> {
    (0..size[1]).flat_map(move |y| (0..size[0]).map(move |x| block_at(origin, side, [x as i32, y as i32])))
}

/// A rectangle of cells, both corners included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    /// The bottom left cell.
    pub min: [i32; 2],
    /// The top right cell.
    pub max: [i32; 2],
}

impl Bounds {
    /// How many cells wide and high.
    pub fn size(&self) -> [u32; 2] {
        [0, 1].map(|i| (self.max[i] - self.min[i] + 1) as u32)
    }
}

/// What a search from one block reached.
struct Walk {
    /// The cells reached, the start's first, nearest first.
    cells: Vec<[i32; 2]>,
    cut_off: bool,
    found: Option<Pos>,
}

/// Visits the screen blocks joined to `start`, nearest first, until `wanted` accepts one.
/// None when `start` is no screen block or `max` has no room for one.
fn walk<B: Blocks + ?Sized>(
    blocks: &B,
    start: Pos,
    side: Side,
    max: [u32; 2],
    mut wanted: impl FnMut(Pos) -> bool,
) -> Option<Walk> {
    if max[0] == 0 || max[1] == 0 || !blocks.is_screen(start) {
        return None;
    }
    let mut walk = Walk { cells: vec![[0, 0]], cut_off: false, found: None };
    // Every cell looked at, screen block or not: each is asked of the world once.
    let mut seen = HashSet::from([[0, 0]]);
    let mut head = 0;
    while head < walk.cells.len() {
        let cell = walk.cells[head];
        head += 1;
        let pos = block_at(start, side, cell);
        if wanted(pos) {
            walk.found = Some(pos);
            break;
        }
        for step in NEIGHBOURS {
            let next = [cell[0] + step[0], cell[1] + step[1]];
            if !seen.insert(next) || !blocks.is_screen(block_at(start, side, next)) {
                continue;
            }
            if next[0].unsigned_abs() < max[0] && next[1].unsigned_abs() < max[1] {
                walk.cells.push(next);
            } else {
                walk.cut_off = true;
            }
        }
    }
    Some(walk)
}

/// The screen blocks joined to one of them, edge to edge or corner to corner, in the plane
/// of one face.
///
/// It is a picture of the world when it was taken: placing or removing a screen block in
/// that plane, within a maximum screen of the start, makes it stale.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    side: Side,
    start: Pos,
    cells: Vec<[i32; 2]>,
    bounds: Bounds,
    cut_off: bool,
}

impl Component {
    /// The blocks joined to `start` on `side`, no further from it than a screen of `max`
    /// blocks (width, height) could reach. None when `start` is no screen block, or `max`
    /// is nothing.
    pub fn find<B: Blocks + ?Sized>(blocks: &B, start: Pos, side: Side, max: [u32; 2]) -> Option<Component> {
        let walk = walk(blocks, start, side, max, |_| false)?;
        // The start is cell (0, 0), so both corners begin there.
        let furthest = |pick: fn(i32, i32) -> i32| {
            [0, 1].map(|axis| walk.cells.iter().map(|cell| cell[axis]).fold(0, pick))
        };
        let bounds = Bounds { min: furthest(i32::min), max: furthest(i32::max) };
        Some(Component { side, start, cells: walk.cells, bounds, cut_off: walk.cut_off })
    }

    /// The face the blocks are seen on.
    pub fn side(&self) -> Side {
        self.side
    }

    /// The block the search started from.
    pub fn start(&self) -> Pos {
        self.start
    }

    /// The rectangle around the blocks, in cells counted from `start`.
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    /// The bottom left corner of the rectangle around the blocks: where a screen made of
    /// them has its cell (0, 0). Not always a screen block.
    pub fn origin(&self) -> Pos {
        block_at(self.start, self.side, self.bounds.min)
    }

    /// The width and height of the rectangle around the blocks.
    pub fn size(&self) -> [u32; 2] {
        self.bounds.size()
    }

    /// How many blocks.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Never: a component holds at least the block it was searched from.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Whether more screen blocks are joined to these beyond what the search may reach.
    /// The origin and the size are then those of a part, and change with the start.
    pub fn is_cut_off(&self) -> bool {
        self.cut_off
    }

    /// Whether these blocks can be one screen of at most `max` blocks (width, height).
    pub fn fits(&self, max: [u32; 2]) -> bool {
        let size = self.size();
        !self.cut_off && size[0] <= max[0] && size[1] <= max[1]
    }

    /// The blocks, the start first, then nearest first. Filter them to find the block that
    /// runs the screen; more than one answer means two screens were joined into one.
    pub fn blocks(&self) -> impl Iterator<Item = Pos> {
        self.cells.iter().map(|&cell| block_at(self.start, self.side, cell))
    }

    /// Whether `pos` is one of the blocks.
    pub fn contains(&self, pos: Pos) -> bool {
        let cell = cell_of(self.start, self.side, pos);
        block_at(self.start, self.side, cell) == pos && self.cells.contains(&cell)
    }

    /// Which cells of the rectangle around the blocks hold one: row by row from `origin`,
    /// cell (x, y) at `y * width + x`.
    pub fn mask(&self) -> Vec<bool> {
        let width = self.size()[0] as usize;
        let mut mask = vec![false; width * self.size()[1] as usize];
        for cell in &self.cells {
            let [x, y] = [0, 1].map(|i| (cell[i] - self.bounds.min[i]) as usize);
            mask[y * width + x] = true;
        }
        mask
    }
}

/// The origin of the screen `start` belongs to on `side`; `start` itself when it is no
/// screen block. See `Component::find` for `max`.
pub fn connected_origin<B: Blocks + ?Sized>(blocks: &B, start: Pos, side: Side, max: [u32; 2]) -> Pos {
    Component::find(blocks, start, side, max).map_or(start, |component| component.origin())
}

/// The nearest block joined to `start` on `side` that `wanted` accepts, `start` included;
/// of several as near, the first in the order of `Component::blocks`. Stops looking as soon
/// as it has one.
///
/// This is how the block that runs a screen is found from any block of it. `wanted` should
/// accept only a block that has a screen on this very side: a block can run screens on
/// other faces, which are other screens.
pub fn nearest_connected<B: Blocks + ?Sized>(
    blocks: &B,
    start: Pos,
    side: Side,
    max: [u32; 2],
    wanted: impl FnMut(Pos) -> bool,
) -> Option<Pos> {
    walk(blocks, start, side, max, wanted)?.found
}

/// The cells a column or a row covers, from `start` up to but not including `end`. In a
/// column `start` is the bottom, in a row the left. Holes in between are covered too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

/// The half of an empty cell that `ShapeMode::SmoothOne` fills, named by the corner of the
/// cell it holds: the one between the two screen blocks the cell touches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Corner {
    BottomLeft,
    TopLeft,
    BottomRight,
    TopRight,
}

impl Corner {
    /// Whether a point of the cell, 0 to 1 each way from its bottom left, is on the filled
    /// half. The slanted edge is part of it.
    pub fn contains(self, point: [f32; 2]) -> bool {
        let [x, y] = point;
        match self {
            Corner::BottomLeft => y <= 1.0 - x,
            Corner::TopLeft => y >= x,
            Corner::BottomRight => y <= x,
            Corner::TopRight => y >= 1.0 - x,
        }
    }

    /// The filled half as a triangle of the cell, 0 to 1 each way from its bottom left. It
    /// starts at the corner it is named after and turns counter-clockwise, `x` drawn to the
    /// right and `y` upwards.
    pub fn triangle(self) -> [[f32; 2]; 3] {
        match self {
            Corner::BottomLeft => [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            Corner::TopLeft => [[0.0, 1.0], [0.0, 0.0], [1.0, 1.0]],
            Corner::BottomRight => [[1.0, 0.0], [1.0, 1.0], [0.0, 0.0]],
            Corner::TopRight => [[1.0, 1.0], [0.0, 1.0], [1.0, 0.0]],
        }
    }
}

/// Where the long slopes of `ShapeMode::SmoothLong` take the edge: for each column or row,
/// the edge at its two ends. None where the line is empty or its edge stays where the
/// blocks end.
#[derive(Clone, Debug, PartialEq)]
struct Slopes {
    top: Vec<Option<[f32; 2]>>,
    bottom: Vec<Option<[f32; 2]>>,
    left: Vec<Option<[f32; 2]>>,
    right: Vec<Option<[f32; 2]>>,
}

/// What a shape mode adds to the staircase of whole blocks.
#[derive(Clone, Debug, PartialEq)]
enum Smoothing {
    None,
    /// For each cell of the rectangle, the half that is filled.
    Corners(Vec<Option<Corner>>),
    Slopes(Slopes),
}

/// What a screen covers: its blocks inside its rectangle, and what its shape mode adds to
/// their edge. Everything is in cells of the rectangle, (0, 0) at the origin.
///
/// It is a plain value made to be kept: compute it once and compare or clone it freely.
/// It stays true until a screen block is placed or removed in the screen's plane within a
/// maximum screen of it (the origin can move then, so find that again first), or until the
/// screen's size or shape mode changes. Nothing else changes it: not the page, not the
/// rotation, not the resolution.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    width: usize,
    height: usize,
    mask: Vec<bool>,
    columns: Vec<Option<Span>>,
    rows: Vec<Option<Span>>,
    smoothing: Smoothing,
}

impl Shape {
    /// The shape of the screen of `size` blocks (width, height) whose cell (0, 0) is
    /// `origin` on `side`, its edge drawn as `mode` says.
    ///
    /// The screen is the blocks joined to `seed` inside the rectangle; `seed` is the block
    /// that runs the screen. Without a seed, or when it is outside the rectangle or no
    /// screen block, the screen is taken from the first screen block of the rectangle, in
    /// the order of `rect_blocks`. A rectangle without any screen block gives an empty shape.
    ///
    /// Every block of the rectangle is asked of the world, so `size` must be one a screen
    /// may have.
    pub fn compute<B: Blocks + ?Sized>(
        blocks: &B,
        origin: Pos,
        side: Side,
        size: [u32; 2],
        mode: ShapeMode,
        seed: Option<Pos>,
    ) -> Shape {
        let [width, height] = size.map(|length| length as usize);
        let raw: Vec<bool> = rect_blocks(origin, side, size).map(|pos| blocks.is_screen(pos)).collect();
        let seeded = seed.and_then(|seed| {
            let [x, y] = cell_of(origin, side, seed).map(|at| usize::try_from(at).ok());
            let index = y? * width + x.filter(|&x| x < width)?;
            raw.get(index).is_some_and(|&screen| screen).then_some(index)
        });
        let start = seeded.or_else(|| raw.iter().position(|&screen| screen));
        Shape::of_cells(width, height, &raw, start, mode)
    }

    /// The shape of the cells of `raw` joined to the one at `start`.
    fn of_cells(width: usize, height: usize, raw: &[bool], start: Option<usize>, mode: ShapeMode) -> Shape {
        let mut mask = vec![false; raw.len()];
        let mut pending = Vec::new();
        if let Some(start) = start {
            mask[start] = true;
            pending.push(start);
        }
        while let Some(index) = pending.pop() {
            let (x, y) = (index % width, index / width);
            for [dx, dy] in NEIGHBOURS {
                let beside = (x.checked_add_signed(dx as isize), y.checked_add_signed(dy as isize));
                let (Some(x), Some(y)) = beside else {
                    continue;
                };
                let next = y * width + x;
                if x < width && y < height && raw[next] && !mask[next] {
                    mask[next] = true;
                    pending.push(next);
                }
            }
        }

        let mut columns = vec![None; width];
        let mut rows = vec![None; height];
        for y in 0..height {
            for x in 0..width {
                if mask[y * width + x] {
                    grow(&mut columns[x], y as u32);
                    grow(&mut rows[y], x as u32);
                }
            }
        }

        let smoothing = match mode {
            ShapeMode::None => Smoothing::None,
            ShapeMode::SmoothOne => Smoothing::Corners(corners(width, height, &mask)),
            ShapeMode::SmoothLong => Smoothing::Slopes(Slopes {
                top: slopes(&columns, true),
                bottom: slopes(&columns, false),
                left: slopes(&rows, false),
                right: slopes(&rows, true),
            }),
        };
        Shape { width, height, mask, columns, rows, smoothing }
    }

    /// The width and height of the screen's rectangle, in blocks.
    pub fn size(&self) -> [u32; 2] {
        [self.width as u32, self.height as u32]
    }

    /// How the edge is drawn.
    pub fn mode(&self) -> ShapeMode {
        match self.smoothing {
            Smoothing::None => ShapeMode::None,
            Smoothing::Corners(_) => ShapeMode::SmoothOne,
            Smoothing::Slopes(_) => ShapeMode::SmoothLong,
        }
    }

    /// Whether the screen has no block at all: there was none in its rectangle.
    pub fn is_empty(&self) -> bool {
        !self.mask.contains(&true)
    }

    /// Whether the cell is a block of the screen. False outside the rectangle.
    pub fn has_block(&self, x: u32, y: u32) -> bool {
        self.index(x, y).is_some_and(|index| self.mask[index])
    }

    /// The cells that are blocks of the screen, row by row from the bottom left. Each shows
    /// its own square of the picture, whatever the shape mode.
    pub fn cells(&self) -> impl Iterator<Item = [u32; 2]> {
        let width = self.width;
        let cell = move |index: usize| [(index % width) as u32, (index / width) as u32];
        self.mask.iter().enumerate().filter(|(_, block)| **block).map(move |(index, _)| cell(index))
    }

    /// From the lowest block of column `x` to above its highest. None for a column without
    /// a block, or outside the rectangle.
    pub fn column(&self, x: u32) -> Option<Span> {
        self.columns.get(x as usize).copied().flatten()
    }

    /// From the leftmost block of row `y` to past its rightmost. None for a row without a
    /// block, or outside the rectangle.
    pub fn row(&self, y: u32) -> Option<Span> {
        self.rows.get(y as usize).copied().flatten()
    }

    /// The half of an empty cell that `ShapeMode::SmoothOne` fills: an empty cell that
    /// touches exactly two blocks of the screen by its sides, one beside it and one above
    /// or below. None for any other cell and in the other modes.
    pub fn corner(&self, x: u32, y: u32) -> Option<Corner> {
        match &self.smoothing {
            Smoothing::Corners(corners) => corners[self.index(x, y)?],
            _ => None,
        }
    }

    /// What `ShapeMode::SmoothLong` adds above column `x`: how high the screen reaches at
    /// the column's left edge and at its right edge, in cells, neither lower than the
    /// column's top (`column(x).end`). The added piece is the four-sided figure between
    /// that top and these two heights. None where nothing is added, and in the other modes.
    pub fn top_slope(&self, x: u32) -> Option<[f32; 2]> {
        self.slope(|slopes| &slopes.top, x)
    }

    /// What `ShapeMode::SmoothLong` adds below column `x`: how low the screen reaches at
    /// the column's left edge and at its right edge, neither higher than the column's
    /// bottom (`column(x).start`). None where nothing is added, and in the other modes.
    pub fn bottom_slope(&self, x: u32) -> Option<[f32; 2]> {
        self.slope(|slopes| &slopes.bottom, x)
    }

    /// What `ShapeMode::SmoothLong` adds left of row `y`: how far left the screen reaches
    /// at the row's bottom edge and at its top edge, neither further right than the row's
    /// left end (`row(y).start`). None where nothing is added, and in the other modes.
    pub fn left_slope(&self, y: u32) -> Option<[f32; 2]> {
        self.slope(|slopes| &slopes.left, y)
    }

    /// What `ShapeMode::SmoothLong` adds right of row `y`: how far right the screen reaches
    /// at the row's bottom edge and at its top edge, neither further left than the row's
    /// right end (`row(y).end`). None where nothing is added, and in the other modes.
    pub fn right_slope(&self, y: u32) -> Option<[f32; 2]> {
        self.slope(|slopes| &slopes.right, y)
    }

    /// Whether a point of the rectangle is on the screen, what the shape mode adds
    /// included. `x` and `y` are in blocks from the origin's corner, so the whole of cell
    /// (2, 0) is 2 <= x < 3, 0 <= y < 1. A slanted edge is part of the screen.
    pub fn is_inside(&self, x: f32, y: f32) -> bool {
        // Written so that NaN is outside.
        if !(x >= 0.0 && y >= 0.0 && x < self.width as f32 && y < self.height as f32) {
            return false;
        }
        let (cell_x, cell_y) = (x.floor() as usize, y.floor() as usize);
        if cell_x >= self.width || cell_y >= self.height {
            return false;
        }
        if self.mask[cell_y * self.width + cell_x] {
            return true;
        }
        let within = [x - cell_x as f32, y - cell_y as f32];
        let between = |ends: [f32; 2], along: f32| ends[0] + (ends[1] - ends[0]) * along;
        match &self.smoothing {
            Smoothing::None => false,
            Smoothing::Corners(corners) => {
                corners[cell_y * self.width + cell_x].is_some_and(|corner| corner.contains(within))
            }
            Smoothing::Slopes(slopes) => {
                // An edge that stays flat is still part of the screen in this mode: a point
                // exactly on it counts, as in the Java mod.
                if let Some(column) = self.columns[cell_x] {
                    let (bottom, top) = (column.start as f32, column.end as f32);
                    let above = slopes.top[cell_x].unwrap_or([top, top]);
                    let below = slopes.bottom[cell_x].unwrap_or([bottom, bottom]);
                    if y >= top && y <= between(above, within[0]) {
                        return true;
                    }
                    if y <= bottom && y >= between(below, within[0]) {
                        return true;
                    }
                }
                if let Some(row) = self.rows[cell_y] {
                    let (left, right) = (row.start as f32, row.end as f32);
                    let before = slopes.left[cell_y].unwrap_or([left, left]);
                    let after = slopes.right[cell_y].unwrap_or([right, right]);
                    if x < left && x >= between(before, within[1]) {
                        return true;
                    }
                    if x >= right && x <= between(after, within[1]) {
                        return true;
                    }
                }
                false
            }
        }
    }

    fn index(&self, x: u32, y: u32) -> Option<usize> {
        let (x, y) = (x as usize, y as usize);
        (x < self.width && y < self.height).then(|| y * self.width + x)
    }

    fn slope(&self, edge: impl Fn(&Slopes) -> &Vec<Option<[f32; 2]>>, line: u32) -> Option<[f32; 2]> {
        match &self.smoothing {
            Smoothing::Slopes(slopes) => edge(slopes).get(line as usize).copied().flatten(),
            _ => None,
        }
    }
}

/// Stretches a column's or a row's span to hold the cell `at`.
fn grow(span: &mut Option<Span>, at: u32) {
    *span = Some(match *span {
        None => Span { start: at, end: at + 1 },
        Some(span) => Span { start: span.start.min(at), end: span.end.max(at + 1) },
    });
}

/// For each cell, the half `ShapeMode::SmoothOne` fills.
fn corners(width: usize, height: usize, mask: &[bool]) -> Vec<Option<Corner>> {
    let block = |x: usize, y: usize| mask[y * width + x];
    let mut corners = vec![None; mask.len()];
    for y in 0..height {
        for x in 0..width {
            if block(x, y) {
                continue;
            }
            let left = x > 0 && block(x - 1, y);
            let right = x + 1 < width && block(x + 1, y);
            let down = y > 0 && block(x, y - 1);
            let up = y + 1 < height && block(x, y + 1);
            corners[y * width + x] = match (left, right, down, up) {
                (true, false, true, false) => Some(Corner::BottomLeft),
                (true, false, false, true) => Some(Corner::TopLeft),
                (false, true, true, false) => Some(Corner::BottomRight),
                (false, true, false, true) => Some(Corner::TopRight),
                _ => None,
            };
        }
    }
    corners
}

/// The long slopes along one edge of a shape. `lines` are its columns (or rows) and `far`
/// says which end of them the edge is: the top of columns and the right of rows are far,
/// the bottom and the left are near. Gives, for each line, the edge at its two ends.
fn slopes(lines: &[Option<Span>], far: bool) -> Vec<Option<[f32; 2]>> {
    let edge = |span: Span| if far { span.end } else { span.start };
    let edges: Vec<Option<u32>> = lines.iter().map(|line| line.map(edge)).collect();
    let forward = run_lines(&edges, far);
    // The same read from the other end, then put back the right way round.
    let reversed: Vec<Option<u32>> = edges.iter().rev().copied().collect();
    let mut backward = run_lines(&reversed, far);
    backward.reverse();

    let further = |edge: f32, line: f32| if far { edge.max(line) } else { edge.min(line) };
    edges
        .iter()
        .enumerate()
        .map(|(at, edge)| {
            let edge = (*edge)? as f32;
            let ends = [further(edge, backward[at]), further(edge, forward[at + 1])];
            (ends != [edge, edge]).then_some(ends)
        })
        .collect()
}

/// One straight line over each run of lines whose edge steps back towards the shape (down
/// for a far edge, up for a near one) or stays level: from the first line's edge at the
/// start of the run to the last line's edge at its end. Gives the line's value at each of
/// the `edges.len() + 1` boundaries between lines; where two runs meet, the later one's.
fn run_lines(edges: &[Option<u32>], far: bool) -> Vec<f32> {
    let mut line = vec![0.0; edges.len() + 1];
    let mut at = 0;
    while at < edges.len() {
        let Some(first) = edges[at] else {
            at += 1;
            continue;
        };
        let (mut end, mut last) = (at, first);
        while let Some(&Some(next)) = edges.get(end + 1)
            && (if far { next <= last } else { next >= last })
        {
            end += 1;
            last = next;
        }
        let span = (end + 1 - at) as f32;
        for (step, value) in line[at..=end + 1].iter_mut().enumerate() {
            *value = first as f32 + (last as f32 - first as f32) * (step as f32 / span);
        }
        at = end + 1;
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: Pos = [10, 20, 30];
    const MAX: [u32; 2] = [16, 16];
    const MODES: [ShapeMode; 3] = ShapeMode::ALL;

    /// The screen blocks of a drawing laid on `side` with its bottom left cell at `origin`:
    /// rows from the top one down, `#` a screen block.
    fn drawn(side: Side, origin: Pos, rows: &[&str]) -> HashSet<Pos> {
        let mut set = HashSet::new();
        for (y, row) in rows.iter().rev().enumerate() {
            for (x, mark) in row.chars().enumerate() {
                if mark == '#' {
                    set.insert(block_at(origin, side, [x as i32, y as i32]));
                }
            }
        }
        set
    }

    fn world(set: &HashSet<Pos>) -> impl Fn(Pos) -> bool {
        move |pos| set.contains(&pos)
    }

    fn size_of(rows: &[&str]) -> [u32; 2] {
        [rows[0].len() as u32, rows.len() as u32]
    }

    /// The shape of a drawing whose rectangle is the drawing's, on a south wall.
    fn shape(rows: &[&str], mode: ShapeMode) -> Shape {
        let set = drawn(Side::South, ORIGIN, rows);
        Shape::compute(&world(&set), ORIGIN, Side::South, size_of(rows), mode, None)
    }

    /// A shape's blocks drawn back, rows from the top one down.
    fn picture(shape: &Shape) -> Vec<String> {
        let [width, height] = shape.size();
        let mark = |x: u32, y: u32| if shape.has_block(x, y) { '#' } else { '.' };
        let row = |y: u32| -> String { (0..width).map(|x| mark(x, y)).collect() };
        (0..height).rev().map(row).collect()
    }

    /// Every cell `SmoothOne` fills half of, with the half.
    fn corners_of(shape: &Shape) -> Vec<([u32; 2], Corner)> {
        let [width, height] = shape.size();
        let cells = (0..height).flat_map(|y| (0..width).map(move |x| [x, y]));
        cells.filter_map(|[x, y]| shape.corner(x, y).map(|corner| ([x, y], corner))).collect()
    }

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5
    }

    fn span(start: u32, end: u32) -> Option<Span> {
        Some(Span { start, end })
    }

    const RECTANGLE: [&str; 2] = ["###", "###"];
    const L: [&str; 3] = ["#..", "#..", "###"];
    const T: [&str; 3] = ["###", ".#.", ".#."];
    const U: [&str; 3] = ["#.#", "#.#", "###"];
    const RING: [&str; 3] = ["###", "#.#", "###"];
    const DIAGONAL: [&str; 3] = ["..#", ".#.", "#.."];
    /// Column tops 4, 3, 2, 1: a staircase going down to the right.
    const STAIRS: [&str; 4] = ["#...", "##..", "###.", "####"];
    /// Column bottoms 0, 1, 2, 3: the same staircase hung from the top.
    const HANGING: [&str; 4] = ["####", "###.", "##..", "#..."];
    /// Row left ends 0, 1, 2, 3: the staircase going up to the right.
    const RISING: [&str; 4] = ["...#", "..##", ".###", "####"];

    #[test]
    fn a_rectangle_is_all_blocks_and_gets_nothing_added_in_any_mode() {
        let set = drawn(Side::South, ORIGIN, &RECTANGLE);
        for start in &set {
            let component = Component::find(&world(&set), *start, Side::South, MAX).unwrap();
            assert_eq!((component.origin(), component.size(), component.len()), (ORIGIN, [3, 2], 6));
            assert!(component.fits(MAX) && !component.is_cut_off());
            assert_eq!(component.mask(), vec![true; 6]);
        }
        for mode in MODES {
            let shape = shape(&RECTANGLE, mode);
            assert_eq!((shape.size(), shape.mode()), ([3, 2], mode));
            assert_eq!(picture(&shape), RECTANGLE);
            assert_eq!(shape.cells().count(), 6);
            assert_eq!((shape.column(2), shape.row(1)), (span(0, 2), span(0, 3)));
            assert_eq!((shape.column(3), shape.row(2)), (None, None), "outside the rectangle");
            assert!(corners_of(&shape).is_empty());
            for line in 0..3 {
                let slopes = [shape.top_slope(line), shape.bottom_slope(line), shape.left_slope(line)];
                assert_eq!(slopes, [None; 3], "{mode:?}");
                assert_eq!(shape.right_slope(line), None, "{mode:?}");
            }
        }
    }

    #[test]
    fn an_l_keeps_its_arm_and_its_foot() {
        let shape = shape(&L, ShapeMode::SmoothOne);
        assert_eq!(picture(&shape), L);
        assert_eq!([shape.column(0), shape.column(1), shape.column(2)], [span(0, 3), span(0, 1), span(0, 1)]);
        assert_eq!([shape.row(0), shape.row(1), shape.row(2)], [span(0, 3), span(0, 1), span(0, 1)]);
        assert_eq!(shape.cells().collect::<Vec<_>>(), [[0, 0], [1, 0], [2, 0], [0, 1], [0, 2]]);
        // Only the cell in the angle touches a block beside it and one below it.
        assert_eq!(corners_of(&shape), [([1, 1], Corner::BottomLeft)]);
    }

    #[test]
    fn a_t_has_its_origin_on_a_cell_that_is_no_screen_block() {
        let set = drawn(Side::South, ORIGIN, &T);
        let foot = block_at(ORIGIN, Side::South, [1, 0]);
        let component = Component::find(&world(&set), foot, Side::South, MAX).unwrap();
        assert_eq!((component.origin(), component.size(), component.len()), (ORIGIN, [3, 3], 5));
        assert!(!set.contains(&ORIGIN) && !component.contains(ORIGIN));
        assert_eq!(component.bounds(), Bounds { min: [-1, 0], max: [1, 2] }, "counted from the foot");
        let mask = [false, true, false, false, true, false, true, true, true];
        assert_eq!(component.mask(), mask, "rows from the bottom");

        let shape = shape(&T, ShapeMode::SmoothOne);
        assert_eq!(picture(&shape), T);
        assert_eq!([shape.column(0), shape.column(1), shape.column(2)], [span(2, 3), span(0, 3), span(2, 3)]);
        assert_eq!([shape.row(0), shape.row(1), shape.row(2)], [span(1, 2), span(1, 2), span(0, 3)]);
        // Under each arm, against the leg.
        assert_eq!(corners_of(&shape), [([0, 1], Corner::TopRight), ([2, 1], Corner::TopLeft)]);
    }

    #[test]
    fn the_gap_of_a_u_is_not_part_of_it() {
        for mode in MODES {
            let shape = shape(&U, mode);
            assert_eq!(picture(&shape), U);
            assert_eq!(shape.column(1), span(0, 1));
            assert_eq!(shape.row(2), span(0, 3), "a row's span covers the gap between its blocks");
            assert!(corners_of(&shape).is_empty(), "a cell between two blocks is no corner");
            assert!(shape.is_inside(0.5, 2.5) && shape.is_inside(2.5, 2.5));
        }
        assert!(!shape(&U, ShapeMode::None).is_inside(1.5, 2.5));
        assert!(!shape(&U, ShapeMode::SmoothOne).is_inside(1.5, 2.5));
        // Long slopes join the two arms over the gap, as in the Java mod: tops 3, 1, 3 make
        // the middle column reach its neighbours' top at both ends.
        let joined = shape(&U, ShapeMode::SmoothLong);
        assert_eq!(joined.top_slope(1), Some([3.0, 3.0]));
        assert!(joined.is_inside(1.5, 2.5) && !joined.has_block(1, 2));
    }

    #[test]
    fn a_hole_in_the_screen_is_not_part_of_it() {
        let set = drawn(Side::South, ORIGIN, &RING);
        let component = Component::find(&world(&set), ORIGIN, Side::South, MAX).unwrap();
        assert_eq!((component.size(), component.len()), ([3, 3], 8));
        assert!(!component.mask()[4] && !component.contains(block_at(ORIGIN, Side::South, [1, 1])));
        for mode in MODES {
            let shape = shape(&RING, mode);
            assert_eq!(picture(&shape), RING);
            assert!(!shape.has_block(1, 1));
            assert_eq!(shape.column(1), span(0, 3), "the span covers the hole, has_block tells it");
            assert_eq!(shape.corner(1, 1), None, "a cell between four blocks is no corner");
            assert!(!shape.is_inside(1.5, 1.5), "{mode:?}");
            assert!(shape.is_inside(1.5, 0.5) && shape.is_inside(0.5, 1.5), "{mode:?}");
        }
    }

    #[test]
    fn blocks_that_touch_by_a_corner_only_are_one_screen() {
        let set = drawn(Side::South, ORIGIN, &DIAGONAL);
        let top = block_at(ORIGIN, Side::South, [2, 2]);
        let component = Component::find(&world(&set), top, Side::South, MAX).unwrap();
        assert_eq!((component.origin(), component.size(), component.len()), (ORIGIN, [3, 3], 3));

        let shape = shape(&DIAGONAL, ShapeMode::SmoothOne);
        assert_eq!(picture(&shape), DIAGONAL);
        // Each cell beside the diagonal touches two of its blocks and gets its half.
        let expected = [
            ([1, 0], Corner::TopLeft),
            ([0, 1], Corner::BottomRight),
            ([2, 1], Corner::TopLeft),
            ([1, 2], Corner::BottomRight),
        ];
        assert_eq!(corners_of(&shape), expected);
    }

    #[test]
    fn blocks_in_front_behind_or_apart_are_not_joined() {
        let mut set = drawn(Side::South, ORIGIN, &RECTANGLE);
        set.insert(offset(ORIGIN, Side::South.forward(), 1));
        set.insert(offset(ORIGIN, Side::South.forward(), -1));
        set.insert(block_at(ORIGIN, Side::South, [5, 0]));
        let component = Component::find(&world(&set), ORIGIN, Side::South, MAX).unwrap();
        assert_eq!((component.size(), component.len()), ([3, 2], 6));
        assert!(!component.contains(offset(ORIGIN, Side::South.forward(), 1)), "in front of a block of it");
        assert!(!component.is_cut_off());
    }

    #[test]
    fn nothing_is_found_from_a_block_that_is_no_screen_or_with_no_room() {
        let set = drawn(Side::South, ORIGIN, &RECTANGLE);
        let beside = block_at(ORIGIN, Side::South, [-1, 0]);
        assert_eq!(Component::find(&world(&set), beside, Side::South, MAX), None);
        assert_eq!(Component::find(&world(&set), ORIGIN, Side::South, [0, 16]), None);
        assert_eq!(Component::find(&world(&set), ORIGIN, Side::South, [16, 0]), None);
        assert_eq!(connected_origin(&world(&set), beside, Side::South, MAX), beside, "left where it was");
        assert_eq!(nearest_connected(&world(&set), beside, Side::South, MAX, |_| true), None);
    }

    #[test]
    fn the_search_stops_a_maximum_screen_away_from_its_start() {
        let row = ["####################"];
        let set = drawn(Side::South, ORIGIN, &row);
        let at = |x: i32| block_at(ORIGIN, Side::South, [x, 0]);

        // From the left end the 16 blocks within reach are found, and more are known to follow.
        let from_end = Component::find(&world(&set), at(0), Side::South, MAX).unwrap();
        assert_eq!((from_end.origin(), from_end.size(), from_end.len()), (at(0), [16, 1], 16));
        assert!(from_end.is_cut_off() && !from_end.fits(MAX), "16 wide, yet not a whole screen");

        // From the middle all 20 are within reach: too wide, and known to be.
        let from_middle = Component::find(&world(&set), at(10), Side::South, MAX).unwrap();
        assert_eq!((from_middle.origin(), from_middle.size(), from_middle.len()), (at(0), [20, 1], 20));
        assert!(!from_middle.is_cut_off() && !from_middle.fits(MAX));

        // A part's origin is where the reach ended, not where the blocks end.
        assert_eq!(connected_origin(&world(&set), at(19), Side::South, MAX), at(4));

        // A smaller maximum reaches less far, each way on its own.
        let small = Component::find(&world(&set), at(10), Side::South, [3, 1]).unwrap();
        assert_eq!((small.origin(), small.size()), (at(8), [5, 1]));
        assert!(small.is_cut_off());
    }

    #[test]
    fn the_height_is_bounded_on_its_own() {
        let set = drawn(Side::South, ORIGIN, &["##", "##", "##", "##", "##"]);
        let component = Component::find(&world(&set), ORIGIN, Side::South, [16, 3]).unwrap();
        assert_eq!((component.size(), component.len()), ([2, 3], 6));
        assert!(component.is_cut_off());
        let whole = Component::find(&world(&set), ORIGIN, Side::South, [2, 5]).unwrap();
        assert!(whole.fits([2, 5]) && !whole.fits([2, 4]) && !whole.fits([1, 5]));
    }

    #[test]
    fn each_side_lays_the_plane_along_its_own_right_and_up() {
        // Where the cells two to the right and two up of the origin are, for someone
        // facing the screen: the floor and the ceiling have north up.
        let expected = [
            (Side::Bottom, [12, 20, 30], [10, 20, 28]),
            (Side::Top, [12, 20, 30], [10, 20, 28]),
            (Side::North, [8, 20, 30], [10, 22, 30]),
            (Side::South, [12, 20, 30], [10, 22, 30]),
            (Side::West, [10, 20, 32], [10, 22, 30]),
            (Side::East, [10, 20, 28], [10, 22, 30]),
        ];
        let flat = shape(&L, ShapeMode::SmoothOne);
        for (side, right, up) in expected {
            assert_eq!(block_at(ORIGIN, side, [2, 0]), right, "{side:?}");
            assert_eq!(block_at(ORIGIN, side, [0, 2]), up, "{side:?}");
            assert_eq!(cell_of(ORIGIN, side, block_at(ORIGIN, side, [2, -3])), [2, -3], "{side:?}");
            assert_eq!(cell_of(ORIGIN, side, offset(right, side.forward(), 4)), [2, 0], "in front of it");

            let set = drawn(side, ORIGIN, &L);
            assert!(set.contains(&right) && set.contains(&up) && set.len() == 5, "{side:?}");
            let component = Component::find(&world(&set), right, side, MAX).unwrap();
            assert_eq!((component.side(), component.start()), (side, right));
            assert_eq!((component.origin(), component.size()), (ORIGIN, [3, 3]), "{side:?}");
            assert_eq!(component.bounds(), Bounds { min: [-2, 0], max: [0, 2] }, "{side:?}");
            assert_eq!(component.blocks().collect::<HashSet<_>>(), set, "{side:?}");

            // Seen from in front, the screen is the same on every side.
            let shape = Shape::compute(&world(&set), ORIGIN, side, [3, 3], ShapeMode::SmoothOne, Some(up));
            assert_eq!(shape, flat, "{side:?}");
        }
    }

    #[test]
    fn a_rectangle_is_walked_row_by_row_from_the_origin() {
        let blocks: Vec<Pos> = rect_blocks(ORIGIN, Side::North, [3, 2]).collect();
        assert_eq!(blocks.len(), 6);
        for (index, pos) in blocks.iter().enumerate() {
            let cell = [index as i32 % 3, index as i32 / 3];
            assert_eq!(*pos, block_at(ORIGIN, Side::North, cell), "cell (x, y) comes at y * width + x");
        }
        assert_eq!(blocks[1], [9, 20, 30], "on a north face, right is west");
        assert_eq!(rect_blocks(ORIGIN, Side::North, [0, 4]).count(), 0);
    }

    #[test]
    fn the_nearest_wanted_block_is_found_and_right_comes_first() {
        let set = drawn(Side::South, ORIGIN, &["###", "###", "###"]);
        let at = |x: i32, y: i32| block_at(ORIGIN, Side::South, [x, y]);
        let find = |wanted: &[Pos]| {
            nearest_connected(&world(&set), at(1, 1), Side::South, MAX, |pos| wanted.contains(&pos))
        };

        assert_eq!(find(&[at(1, 1), at(2, 1)]), Some(at(1, 1)), "the start itself is nearest");
        assert_eq!(find(&[at(0, 1), at(2, 1)]), Some(at(2, 1)), "of two as near, the one to the right");
        assert_eq!(find(&[at(1, 0), at(1, 2)]), Some(at(1, 2)), "up before down");
        assert_eq!(find(&[at(0, 0), at(1, 0)]), Some(at(1, 0)), "beside before diagonal");
        assert_eq!(find(&[]), None);
        assert_eq!(find(&[at(5, 5)]), None, "not joined");

        // The whole component tells when two blocks claim the same screen.
        let component = Component::find(&world(&set), at(1, 1), Side::South, MAX).unwrap();
        let claims: Vec<Pos> = component.blocks().filter(|pos| [at(0, 1), at(2, 1)].contains(pos)).collect();
        assert_eq!(claims, [at(2, 1), at(0, 1)]);
        assert_eq!(component.blocks().next(), Some(at(1, 1)));
    }

    #[test]
    fn the_search_stops_asking_once_it_has_its_block() {
        let set = drawn(Side::South, ORIGIN, &["################"]);
        let asked = std::cell::Cell::new(0);
        let counting = |pos: Pos| {
            asked.set(asked.get() + 1);
            set.contains(&pos)
        };
        assert_eq!(nearest_connected(&counting, ORIGIN, Side::South, MAX, |pos| pos == ORIGIN), Some(ORIGIN));
        assert_eq!(asked.get(), 1, "only the start was looked at");
    }

    #[test]
    fn the_seed_chooses_which_piece_of_the_rectangle_is_the_screen() {
        let rows = ["##.##", "##.##"];
        let set = drawn(Side::South, ORIGIN, &rows);
        let at = |x: i32, y: i32| block_at(ORIGIN, Side::South, [x, y]);
        let with = |seed: Option<Pos>| {
            picture(&Shape::compute(&world(&set), ORIGIN, Side::South, [5, 2], ShapeMode::None, seed))
        };
        assert_eq!(with(Some(at(4, 1))), ["...##", "...##"], "the piece the seed is in");
        assert_eq!(with(Some(at(0, 0))), ["##...", "##..."]);
        // Without a usable seed, the first screen block of the rectangle from the origin.
        assert_eq!(with(None), ["##...", "##..."]);
        assert_eq!(with(Some(at(2, 0))), ["##...", "##..."], "a seed that is no screen block");
        assert_eq!(with(Some(at(5, 0))), ["##...", "##..."], "a seed right of the rectangle");
        assert_eq!(with(Some(at(4, 2))), ["##...", "##..."], "a seed above the rectangle");
        assert_eq!(with(Some(at(-1, 0))), ["##...", "##..."], "a seed left of the rectangle");
        let in_front = offset(at(4, 0), Side::South.forward(), 1);
        assert_eq!(with(Some(in_front)), ["...##", "...##"], "a seed in front of a block of the piece");
    }

    #[test]
    fn without_a_seed_the_first_block_from_the_bottom_row_is_taken() {
        // The origin is empty: the bottom row is read first, left to right.
        let shape = shape(&["#....", "...##"], ShapeMode::None);
        assert_eq!(picture(&shape), [".....", "...##"]);
        assert_eq!((shape.column(0), shape.column(3), shape.row(1)), (None, span(0, 1), None));
    }

    #[test]
    fn a_rectangle_without_a_screen_block_gives_an_empty_shape() {
        for mode in MODES {
            let empty = Shape::compute(&|_: Pos| false, ORIGIN, Side::Top, [3, 2], mode, Some(ORIGIN));
            assert!(empty.is_empty() && empty.cells().next().is_none());
            assert_eq!((empty.size(), empty.column(0), empty.row(0)), ([3, 2], None, None));
            assert!(!empty.is_inside(0.5, 0.5));
            let nothing = Shape::compute(&|_: Pos| true, ORIGIN, Side::Top, [0, 0], mode, None);
            assert!(nothing.is_empty() && !nothing.is_inside(0.0, 0.0));
        }
    }

    #[test]
    fn blocks_joined_only_outside_the_rectangle_are_left_out() {
        // A bridge above the rectangle joins its two ends: it is one component, but a
        // screen one block high does not hold the bridge, so it keeps one end only.
        let set = drawn(Side::South, ORIGIN, &[".#.", "#.#"]);
        let component = Component::find(&world(&set), ORIGIN, Side::South, MAX).unwrap();
        assert_eq!((component.size(), component.len()), ([3, 2], 3));
        let low = Shape::compute(&world(&set), ORIGIN, Side::South, [3, 1], ShapeMode::None, None);
        assert_eq!(picture(&low), ["#.."]);
        // A rectangle larger than the blocks changes nothing but its size.
        let large = Shape::compute(&world(&set), ORIGIN, Side::South, [5, 4], ShapeMode::None, None);
        assert_eq!(picture(&large), [".....", ".....", ".#...", "#.#.."]);
    }

    #[test]
    fn a_staircase_stays_a_staircase_without_smoothing() {
        let shape = shape(&STAIRS, ShapeMode::None);
        assert_eq!(picture(&shape), STAIRS);
        let tops: Vec<_> = (0..4).map(|x| shape.column(x)).collect();
        assert_eq!(tops, [span(0, 4), span(0, 3), span(0, 2), span(0, 1)]);
        let ends: Vec<_> = (0..4).map(|y| shape.row(y)).collect();
        assert_eq!(ends, [span(0, 4), span(0, 3), span(0, 2), span(0, 1)]);
        assert!(corners_of(&shape).is_empty());
        assert!((0..4).all(|line| shape.top_slope(line).is_none() && shape.right_slope(line).is_none()));
        // Just above a step: outside, however close to the corner.
        assert!(!shape.is_inside(1.01, 3.01) && !shape.is_inside(1.5, 3.5));
        assert!(shape.is_inside(1.5, 2.99) && shape.is_inside(0.0, 0.0));
    }

    #[test]
    fn smooth_one_fills_half_of_the_cell_in_each_step() {
        let stairs = shape(&STAIRS, ShapeMode::SmoothOne);
        assert_eq!(picture(&stairs), STAIRS, "the blocks are the same in every mode");
        let expected =
            [([3, 1], Corner::BottomLeft), ([2, 2], Corner::BottomLeft), ([1, 3], Corner::BottomLeft)];
        assert_eq!(corners_of(&stairs), expected);
        assert_eq!(stairs.top_slope(1), None, "long slopes are another mode's");

        // The staircase hung from the top fills the upper halves instead.
        let hanging = shape(&HANGING, ShapeMode::SmoothOne);
        let expected = [([1, 0], Corner::TopLeft), ([2, 1], Corner::TopLeft), ([3, 2], Corner::TopLeft)];
        assert_eq!(corners_of(&hanging), expected);
        let rising = shape(&RISING, ShapeMode::SmoothOne);
        let expected =
            [([0, 1], Corner::BottomRight), ([1, 2], Corner::BottomRight), ([2, 3], Corner::BottomRight)];
        assert_eq!(corners_of(&rising), expected);
    }

    #[test]
    fn a_point_is_on_a_smoothed_step_up_to_its_slanted_edge() {
        let shape = shape(&STAIRS, ShapeMode::SmoothOne);
        // Cell (1, 3) is filled below the line from its top left to its bottom right.
        assert!(shape.is_inside(1.25, 3.25), "inside the filled half");
        assert!(shape.is_inside(1.5, 3.5), "on the slanted edge");
        assert!(shape.is_inside(1.0, 3.99) && shape.is_inside(1.99, 3.0), "at its two ends");
        assert!(!shape.is_inside(1.75, 3.75), "past the edge");
        assert!(!shape.is_inside(2.5, 3.5), "an empty cell that is no corner");
        assert!(shape.is_inside(0.5, 3.5) && shape.is_inside(1.5, 2.5), "whole blocks");
    }

    #[test]
    fn points_outside_the_rectangle_are_outside_the_screen() {
        for mode in MODES {
            let shape = shape(&STAIRS, mode);
            assert!(shape.is_inside(0.0, 0.0) && shape.is_inside(3.999, 0.999), "{mode:?}");
            let beyond = [[-0.001, 0.5], [0.5, -0.001], [4.0, 0.5], [0.5, 4.0]];
            let nowhere = [[f32::NAN, 0.5], [0.5, f32::NAN], [f32::INFINITY, 0.5], [0.5, f32::NEG_INFINITY]];
            for [x, y] in beyond.into_iter().chain(nowhere) {
                assert!(!shape.is_inside(x, y), "{mode:?} ({x}, {y})");
            }
        }
    }

    #[test]
    fn a_corner_is_the_triangle_it_says_and_holds_its_own_edge() {
        let cases = [
            (Corner::BottomLeft, [0.0, 0.0], [1.0, 1.0]),
            (Corner::TopLeft, [0.0, 1.0], [1.0, 0.0]),
            (Corner::BottomRight, [1.0, 0.0], [0.0, 1.0]),
            (Corner::TopRight, [1.0, 1.0], [0.0, 0.0]),
        ];
        for (corner, held, opposite) in cases {
            let triangle = corner.triangle();
            assert_eq!(triangle[0], held, "{corner:?} starts at its corner");
            assert!(triangle.iter().all(|&point| corner.contains(point)), "{corner:?}");
            assert!(corner.contains([0.5, 0.5]), "{corner:?}: the slanted edge is part of it");
            assert!(!corner.contains(opposite), "{corner:?}");
            let centre = [0, 1].map(|i| (triangle[0][i] + triangle[1][i] + triangle[2][i]) / 3.0);
            assert!(corner.contains(centre), "{corner:?}");
            // Counter-clockwise: the cross product of its first two edges points at the viewer.
            let edge = |to: usize| [triangle[to][0] - triangle[0][0], triangle[to][1] - triangle[0][1]];
            assert!(edge(1)[0] * edge(2)[1] - edge(1)[1] * edge(2)[0] > 0.0, "{corner:?}");
        }
    }

    #[test]
    fn smooth_long_slopes_a_one_block_staircase_like_smooth_one() {
        // By hand from the Java: the line over the whole run of tops 4, 3, 2, 1 runs from 4
        // to 1 and passes under every step, so a column's right end stays at its own top;
        // read from the right each column is its own run, so a column's left end is its
        // left neighbour's top.
        let shape = shape(&STAIRS, ShapeMode::SmoothLong);
        assert_eq!(picture(&shape), STAIRS);
        let tops: Vec<_> = (0..4).map(|x| shape.top_slope(x)).collect();
        assert_eq!(tops, [None, Some([4.0, 3.0]), Some([3.0, 2.0]), Some([2.0, 1.0])]);
        // The same steps seen from the side: each row reaches its lower neighbour's end at
        // its bottom edge.
        let rights: Vec<_> = (0..4).map(|y| shape.right_slope(y)).collect();
        assert_eq!(rights, [None, Some([4.0, 3.0]), Some([3.0, 2.0]), Some([2.0, 1.0])]);
        for line in 0..4 {
            assert_eq!((shape.bottom_slope(line), shape.left_slope(line)), (None, None), "flat edges");
        }
        assert_eq!((shape.top_slope(4), shape.right_slope(4)), (None, None), "outside the rectangle");
        assert!(corners_of(&shape).is_empty(), "half cells are another mode's");
    }

    #[test]
    fn smooth_long_slopes_the_bottom_and_the_left_too() {
        // Bottoms 0, 1, 2, 3: each column's left end drops to its left neighbour's bottom.
        let hanging = shape(&HANGING, ShapeMode::SmoothLong);
        let bottoms: Vec<_> = (0..4).map(|x| hanging.bottom_slope(x)).collect();
        assert_eq!(bottoms, [None, Some([0.0, 1.0]), Some([1.0, 2.0]), Some([2.0, 3.0])]);
        let rights: Vec<_> = (0..4).map(|y| hanging.right_slope(y)).collect();
        assert_eq!(rights, [Some([1.0, 2.0]), Some([2.0, 3.0]), Some([3.0, 4.0]), None]);
        assert!((0..4).all(|line| hanging.top_slope(line).is_none() && hanging.left_slope(line).is_none()));

        // Left ends 0, 1, 2, 3: each row's bottom edge reaches its lower neighbour's end.
        let rising = shape(&RISING, ShapeMode::SmoothLong);
        let lefts: Vec<_> = (0..4).map(|y| rising.left_slope(y)).collect();
        assert_eq!(lefts, [None, Some([0.0, 1.0]), Some([1.0, 2.0]), Some([2.0, 3.0])]);
        let tops: Vec<_> = (0..4).map(|x| rising.top_slope(x)).collect();
        assert_eq!(tops, [Some([1.0, 2.0]), Some([2.0, 3.0]), Some([3.0, 4.0]), None]);
    }

    #[test]
    fn smooth_long_draws_a_long_step_as_the_java_mod_did() {
        // Tops 3, 3, 3, 1, 1, 1. By hand from the Java: the line over the whole run falls
        // from 3 at the left to 1 at the right, 3 - 2j/6 at boundary j. A column's right end
        // is the higher of its top and that line; its left end is its left neighbour's top
        // where that is higher, else its own.
        let rows = ["###...", "###...", "######"];
        let long = shape(&rows, ShapeMode::SmoothLong);
        let line = |boundary: f32| 3.0 + (1.0 - 3.0) * (boundary / 6.0);
        for flat in [0, 1, 2, 5] {
            assert_eq!(long.top_slope(flat), None, "column {flat}");
        }
        assert_eq!(long.top_slope(3), Some([3.0, line(4.0)]), "from the step down to the line");
        assert_eq!(long.top_slope(4), Some([1.0, line(5.0)]), "from its own top up to the line");
        assert!(close(long.top_slope(3).unwrap(), [3.0, 5.0 / 3.0]));
        assert!(close(long.top_slope(4).unwrap(), [1.0, 4.0 / 3.0]));
        // Row right ends 6, 3, 3: the line runs from 6 to 3 over the three rows.
        let rights = [long.right_slope(0), long.right_slope(1), long.right_slope(2)];
        assert_eq!(rights, [None, Some([6.0, 4.0]), None]);

        // The mirrored screen gets the mirrored slopes.
        let mirrored = shape(&["...###", "...###", "######"], ShapeMode::SmoothLong);
        for x in 0..6 {
            let there = long.top_slope(5 - x).map(|[left, right]| [right, left]);
            assert_eq!(mirrored.top_slope(x), there, "column {x}");
        }
        assert_eq!(mirrored.left_slope(1), Some([0.0, 2.0]));
    }

    #[test]
    fn a_step_up_of_several_blocks_is_one_steep_slope() {
        // Tops 1, 3: the low column climbs to its neighbour's top over its own width.
        let shape = shape(&[".#", ".#", "##"], ShapeMode::SmoothLong);
        assert_eq!((shape.top_slope(0), shape.top_slope(1)), (Some([1.0, 3.0]), None));
        assert!(shape.is_inside(0.5, 1.9) && !shape.is_inside(0.5, 2.1), "the slope is at 2 halfway");
        assert!(shape.is_inside(0.9, 2.7) && !shape.is_inside(0.1, 1.3));
    }

    #[test]
    fn a_point_is_on_a_long_slope_up_to_its_edge() {
        let stairs = shape(&STAIRS, ShapeMode::SmoothLong);
        // Above column 1 the edge falls from 4 to 3: 3.75 a quarter of the way.
        assert!(stairs.is_inside(1.25, 3.25) && stairs.is_inside(1.25, 3.75));
        assert!(!stairs.is_inside(1.25, 3.76));
        assert!(stairs.is_inside(1.5, 3.5), "on the slanted edge");
        assert!(!stairs.is_inside(1.75, 3.75), "past it");
        assert!(!stairs.is_inside(2.5, 3.5), "above another step");
        assert!(stairs.is_inside(0.5, 3.5) && stairs.is_inside(3.5, 0.5), "whole blocks");

        // Below a hanging staircase and left of a rising one.
        let hanging = shape(&HANGING, ShapeMode::SmoothLong);
        assert!(hanging.is_inside(1.25, 0.75) && !hanging.is_inside(1.75, 0.25));
        let rising = shape(&RISING, ShapeMode::SmoothLong);
        assert!(rising.is_inside(0.75, 1.25) && !rising.is_inside(0.25, 1.75));
    }

    #[test]
    fn a_shape_is_a_value_that_changes_only_with_the_blocks_the_size_or_the_mode() {
        let mut set = drawn(Side::South, ORIGIN, &STAIRS);
        let compute = |set: &HashSet<Pos>, size, mode| {
            Shape::compute(&world(set), ORIGIN, Side::South, size, mode, None)
        };
        let kept = compute(&set, [4, 4], ShapeMode::SmoothLong);
        assert_eq!(kept.clone(), kept);
        assert_eq!(compute(&set, [4, 4], ShapeMode::SmoothLong), kept, "the same world gives the same shape");
        assert_ne!(compute(&set, [4, 4], ShapeMode::SmoothOne), kept);
        assert_ne!(compute(&set, [4, 3], ShapeMode::SmoothLong), kept);

        // A block elsewhere changes nothing; one in the screen's plane does.
        set.insert(offset(ORIGIN, Side::South.forward(), 1));
        set.insert(block_at(ORIGIN, Side::South, [9, 9]));
        assert_eq!(compute(&set, [4, 4], ShapeMode::SmoothLong), kept);
        set.insert(block_at(ORIGIN, Side::South, [1, 3]));
        assert_ne!(compute(&set, [4, 4], ShapeMode::SmoothLong), kept);
    }
}
