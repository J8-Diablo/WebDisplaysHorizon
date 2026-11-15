//! A screen as quads in the world: where each block of it is, and which part of the
//! browser's picture it shows. Plain arithmetic on the model's types, so it is tested
//! without the game; `render/gpu.rs` draws what this gives.
use super::geometry::Quad;
use super::glow::{CELLS, Glow, cell_centre};
use crate::model::groups;
use crate::model::hit::{Tile, block_point, face_to_picture, picture_to_face};
use crate::model::letterbox::{BEZEL, Fit};
use crate::model::shape::Shape;
use crate::model::side::{Pos, Rotation, Side};

/// How far in front of the blocks' face the picture is drawn, in blocks: enough that the
/// face behind it never shows through, too little to be seen from the side.
const LIFT: f32 = 0.005;

/// What the quads of a screen are made from.
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    /// The block at the screen's lower-left corner on its face (cell 0, 0).
    pub origin: Pos,
    pub side: Side,
    pub rotation: Rotation,
    /// The screen's extent in blocks, along the side's right and up.
    pub size: [u32; 2],
    /// The browser whose picture it shows.
    pub page: u32,
    /// The part of that picture it shows, for a screen that shares one with others of its
    /// link; None for a screen that shows all of its own.
    pub tile: Option<Tile>,
    /// Where the picture sits in the screen, for a shared browser's picture, which keeps
    /// its own shape; None for a picture that fills the screen.
    pub fit: Option<Fit>,
}

impl Placement {
    /// A point of the face in the world. `at` is in blocks along the side's right and up
    /// from the screen's lower-left corner, counted on the whole blocks.
    fn world(&self, at: [f32; 2]) -> [f64; 3] {
        // The picture is drawn inside the bezel: the grid of blocks is shrunk to fit it, as
        // the Java mod does, so that a cell's picture stays on its own block.
        let scaled = [0, 1].map(|i| BEZEL + at[i] * (self.size[i] as f32 - 2.0 * BEZEL) / self.size[i].max(1) as f32);
        let local = block_point(self.side, scaled);
        let forward = self.side.forward();
        [0, 1, 2].map(|i| f64::from(self.origin[i]) + f64::from(local[i]) + f64::from(forward[i] as f32 * LIFT))
    }

    /// Where the point `at` (as for `world`) is in the picture, 0 to 1 across and down.
    fn picture(&self, at: [f32; 2]) -> [f32; 2] {
        if let Some(tile) = &self.tile {
            return groups::point_uv(self.side, self.rotation, tile, at);
        }
        let on_screen = [0, 1].map(|i| at[i] / self.size[i].max(1) as f32);
        let whole = face_to_picture(self.side, self.rotation, on_screen);
        match &self.fit {
            Some(fit) => fit.to_content_or_bar(whole),
            None => whole,
        }
    }

    /// The quad over the part of the face from `low` to `high` (blocks along right and up).
    fn quad(&self, low: [f32; 2], high: [f32; 2]) -> Quad {
        // Seen from the front: top-left, top-right, bottom-right, bottom-left.
        let mut points = [[low[0], high[1]], [high[0], high[1]], [high[0], low[1]], [low[0], low[1]]];
        // Underneath a block the face's right and up are the ones seen from above, so from
        // below the plane is mirrored: taken the other way round, the quad faces the viewer.
        if self.side == Side::Bottom {
            points.reverse();
        }
        Quad { corners: points.map(|point| self.world(point)), uv: points.map(|point| self.picture(point)), page: self.page }
    }

    /// The quad over any four-sided piece of the face: `points` (blocks along right and up)
    /// go round it clockwise as seen from the front, starting anywhere. A triangle gives one
    /// of its corners twice.
    fn piece(&self, mut points: [[f32; 2]; 4]) -> Quad {
        if self.side == Side::Bottom {
            points.reverse();
        }
        Quad { corners: points.map(|point| self.world(point)), uv: points.map(|point| self.picture(point)), page: self.page }
    }
}

/// The lights a screen throws (`render/glow.rs`): one in the middle of each cell of its
/// picture, wherever the picture's rotation puts that cell on the face.
pub fn glows(placement: &Placement) -> Vec<Glow> {
    let [width, height] = placement.size.map(|blocks| blocks as f32);
    let forward = placement.side.forward().map(|part| part as f32);
    // A wall of screens lights a hall; one block lights its corner.
    let reach = 3.0 + 1.5 * width.max(height);
    (0..CELLS)
        .map(|cell| {
            let on_face = picture_to_face(placement.side, placement.rotation, cell_centre(cell));
            Glow { page: placement.page, cell, position: placement.world([on_face[0] * width, on_face[1] * height]), normal: forward, reach }
        })
        .collect()
}

/// The quads of a screen: one for each block of its shape. A screen whose shape has no
/// block gives none.
pub fn quads(placement: &Placement, shape: &Shape) -> Vec<Quad> {
    let mut all: Vec<Quad> = shape.cells().map(|[x, y]| placement.quad([x as f32, y as f32], [x as f32 + 1.0, y as f32 + 1.0])).collect();
    let [width, height] = shape.size();
    // What the shape mode adds to the staircase of blocks: the picture goes on over the
    // empty half-cells, past the blocks themselves.
    for y in 0..height {
        for x in 0..width {
            // A cut corner: half an empty cell, a triangle. Its corners are given
            // counter-clockwise; the renderer wants them clockwise.
            if let Some(corner) = shape.corner(x, y) {
                let [a, b, c] = corner.triangle().map(|[px, py]| [x as f32 + px, y as f32 + py]);
                all.push(placement.piece([a, c, b, b]));
            }
        }
    }
    for x in 0..width {
        let (left, right) = (x as f32, x as f32 + 1.0);
        let Some(column) = shape.column(x) else { continue };
        if let Some([at_left, at_right]) = shape.top_slope(x) {
            let top = column.end as f32;
            all.push(placement.piece([[left, at_left], [right, at_right], [right, top], [left, top]]));
        }
        if let Some([at_left, at_right]) = shape.bottom_slope(x) {
            let bottom = column.start as f32;
            all.push(placement.piece([[left, bottom], [right, bottom], [right, at_right], [left, at_left]]));
        }
    }
    for y in 0..height {
        let (bottom, top) = (y as f32, y as f32 + 1.0);
        let Some(row) = shape.row(y) else { continue };
        if let Some([at_bottom, at_top]) = shape.left_slope(y) {
            let start = row.start as f32;
            all.push(placement.piece([[at_top, top], [start, top], [start, bottom], [at_bottom, bottom]]));
        }
        if let Some([at_bottom, at_top]) = shape.right_slope(y) {
            let end = row.end as f32;
            all.push(placement.piece([[end, top], [at_top, top], [at_bottom, bottom], [end, bottom]]));
        }
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::shape::Component;
    use crate::model::side::ShapeMode;
    use std::collections::HashSet;

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-4)
    }

    fn close2(a: [f32; 2], b: [f32; 2]) -> bool {
        (0..2).all(|i| (a[i] - b[i]).abs() < 1e-5)
    }

    /// A screen over `blocks`, seen on `side`, as the game would build it.
    fn screen(blocks: &[Pos], side: Side, rotation: Rotation) -> (Placement, Shape) {
        let world: HashSet<Pos> = blocks.iter().copied().collect();
        let is_screen = |pos: Pos| world.contains(&pos);
        let component = Component::find(&is_screen, blocks[0], side, [16, 16]).unwrap();
        let (origin, size) = (component.origin(), component.size());
        let shape = Shape::compute(&is_screen, origin, side, size, ShapeMode::None, Some(blocks[0]));
        (Placement { origin, side, rotation, size, page: 9, tile: None, fit: None }, shape)
    }

    /// The side a quad is seen from, as the renderer reads the order of its corners: against
    /// the way its corners turn. Summed over the whole outline, so that a triangle (a corner
    /// given twice) is read as well as a square.
    fn seen_from(quad: &Quad) -> [f64; 3] {
        let from_first = |index: usize| [0, 1, 2].map(|i| quad.corners[index][i] - quad.corners[0][i]);
        let cross = |u: [f64; 3], v: [f64; 3]| [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let (first, second) = (cross(from_first(1), from_first(2)), cross(from_first(2), from_first(3)));
        [0, 1, 2].map(|i| -(first[i] + second[i]))
    }

    #[test]
    fn a_smoothed_edge_is_drawn_over_the_empty_half_cells_and_faces_the_viewer() {
        // An L: three blocks along the bottom, one above the first. Seen from the south.
        let blocks: [Pos; 4] = [[0, 64, 5], [1, 64, 5], [2, 64, 5], [0, 65, 5]];
        let world: HashSet<Pos> = blocks.iter().copied().collect();
        let is_screen = |pos: Pos| world.contains(&pos);
        let component = Component::find(&is_screen, blocks[0], Side::South, [16, 16]).unwrap();
        let (origin, size) = (component.origin(), component.size());
        let placement = Placement { origin, side: Side::South, rotation: Rotation::R0, size, page: 9, tile: None, fit: None };
        let plain = quads(&placement, &Shape::compute(&is_screen, origin, Side::South, size, ShapeMode::None, Some(blocks[0])));
        assert_eq!(plain.len(), 4, "one quad a block and nothing more");
        for mode in [ShapeMode::SmoothOne, ShapeMode::SmoothLong] {
            let shape = Shape::compute(&is_screen, origin, Side::South, size, mode, Some(blocks[0]));
            let smoothed = quads(&placement, &shape);
            assert!(smoothed.len() > plain.len(), "{mode:?} adds to the staircase");
            for quad in &smoothed {
                assert!(seen_from(quad)[2] > 0.0, "{mode:?}: every piece is seen from the south: {quad:?}");
            }
            // What is added lies over cells that hold no block: the notch of the L.
            for quad in &smoothed[plain.len()..] {
                let middle = [0, 1].map(|i| quad.corners.iter().map(|corner| corner[i]).sum::<f64>() / 4.0);
                assert!(middle[0] > 0.9 && middle[1] > 64.9, "{mode:?}: {middle:?}");
            }
        }
    }

    #[test]
    fn a_screen_throws_its_light_from_each_part_of_its_picture() {
        // Four blocks wide, two high, at z = 5, seen from the south.
        let blocks: Vec<Pos> = (0..4).flat_map(|x| [[x, 64, 5], [x, 65, 5]]).collect();
        let (placement, _) = screen(&blocks, Side::South, Rotation::R0);
        let lights = glows(&placement);
        assert_eq!(lights.len(), CELLS);
        assert!(lights.iter().all(|light| light.normal == [0.0, 0.0, 1.0] && light.page == 9 && light.reach == 9.0));
        // Cell 0 is the picture's top-left: west (low x) and high.
        let (top_left, bottom_right) = (lights[0].position, lights[CELLS - 1].position);
        assert!(top_left[0] < 2.0 && top_left[1] > 65.0, "{top_left:?}");
        assert!(bottom_right[0] > 2.0 && bottom_right[1] < 65.0, "{bottom_right:?}");
        // Turned upside down, the picture's top-left is at the face's bottom-right.
        let turned = glows(&Placement { rotation: Rotation::R180, ..placement });
        assert!(close(turned[0].position, bottom_right), "{:?}", turned[0].position);
    }

    #[test]
    fn a_shared_picture_keeps_its_shape_and_leaves_bars() {
        // A screen four wide and one high showing a square picture: bars left and right.
        let blocks: Vec<Pos> = (0..4).map(|x| [x, 64, 5]).collect();
        let (placement, shape) = screen(&blocks, Side::South, Rotation::R0);
        let fit = Fit::for_screen(placement.size, false, [1000, 1000]);
        let fitted = quads(&Placement { fit: Some(fit), ..placement }, &shape);
        let across: Vec<f32> = fitted.iter().flat_map(|quad| quad.uv.map(|uv| uv[0])).collect();
        let (least, most) = (across.iter().cloned().fold(f32::MAX, f32::min), across.iter().cloned().fold(f32::MIN, f32::max));
        assert!(least < -1.0 && most > 2.0, "the ends of the screen are past the picture: {least} {most}");
        let down: Vec<f32> = fitted.iter().flat_map(|quad| quad.uv.map(|uv| uv[1])).collect();
        assert!(down.iter().all(|v| (-0.001..=1.001).contains(v)), "it fills the height");
        // The middle of the screen is the middle of the picture.
        let middle = Placement { fit: Some(fit), ..placement }.picture([2.0, 0.5]);
        assert!(close2(middle, [0.5, 0.5]), "{middle:?}");
    }

    #[test]
    fn a_wall_screen_covers_its_blocks_inside_the_bezel() {
        // Two blocks side by side at z = 5, seen from the south (the +Z side).
        let (placement, shape) = screen(&[[0, 64, 5], [1, 64, 5]], Side::South, Rotation::R0);
        let quads = quads(&placement, &shape);
        assert_eq!(quads.len(), 2);
        let z = 6.0 + f64::from(LIFT);
        let (bezel, cell) = (f64::from(BEZEL), (2.0 - 2.0 * f64::from(BEZEL)) / 2.0);
        // The first block: its picture starts a bezel in from the screen's corner.
        assert!(close(quads[0].corners[3], [bezel, 64.0 + bezel, z]), "bottom-left: {:?}", quads[0].corners[3]);
        assert!(close(quads[0].corners[0], [bezel, 65.0 - bezel, z]), "top-left");
        assert!(close(quads[0].corners[1], [bezel + cell, 65.0 - bezel, z]), "top-right");
        // The second block ends a bezel short of the far edge.
        assert!(close(quads[1].corners[2], [2.0 - bezel, 64.0 + bezel, z]), "bottom-right of the second");
        // The left block shows the left half of the picture, the right block the right half.
        assert!(close2(quads[0].uv[0], [0.0, 0.0]) && close2(quads[0].uv[2], [0.5, 1.0]));
        assert!(close2(quads[1].uv[0], [0.5, 0.0]) && close2(quads[1].uv[2], [1.0, 1.0]));
        assert!(quads.iter().all(|quad| quad.page == 9));
    }

    #[test]
    fn every_side_faces_whoever_stands_in_front_of_it() {
        for side in Side::ALL {
            let (placement, shape) = screen(&[[10, 70, 10]], side, Rotation::R0);
            let quad = quads(&placement, &shape)[0];
            let [a, b, c] = [quad.corners[0], quad.corners[1], quad.corners[2]];
            let edge = |from: [f64; 3], to: [f64; 3]| [0, 1, 2].map(|i| to[i] - from[i]);
            let (u, v) = (edge(a, b), edge(b, c));
            let normal = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            // Corners taken clockwise as seen from the front: their normal points into the
            // block, away from the viewer, on every side alike.
            let forward = side.forward().map(f64::from);
            let facing: f64 = (0..3).map(|i| normal[i] * forward[i]).sum();
            assert!(facing < 0.0, "{side:?}: {normal:?}");
            // And the quad lies just in front of the block's face.
            let centre = [10.5, 70.5, 10.5];
            let out: f64 = (0..3).map(|i| (a[i] - centre[i]) * forward[i]).sum();
            assert!((out - 0.5 - f64::from(LIFT)).abs() < 1e-4, "{side:?}: {out}");
        }
    }

    #[test]
    fn a_quarter_turn_turns_the_picture_not_the_blocks() {
        let blocks = [[0, 64, 5], [1, 64, 5]];
        let upright = quads(&screen(&blocks, Side::South, Rotation::R0).0, &screen(&blocks, Side::South, Rotation::R0).1);
        let (placement, shape) = screen(&blocks, Side::South, Rotation::R90);
        let turned = quads(&placement, &shape);
        assert_eq!(turned[0].corners, upright[0].corners, "the blocks are where they were");
        assert_ne!(turned[0].uv, upright[0].uv);
        // Whatever the turn, the two blocks share the picture between them without overlap.
        let area = |quad: &Quad| {
            let (us, vs): (Vec<f32>, Vec<f32>) = quad.uv.iter().map(|uv| (uv[0], uv[1])).unzip();
            let span = |v: &[f32]| v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min);
            span(&us) * span(&vs)
        };
        assert!((area(&turned[0]) + area(&turned[1]) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn a_hole_in_the_screen_has_no_quad() {
        // A ring of eight blocks round an empty middle.
        let ring: Vec<Pos> = (0..3).flat_map(|x| (0..3).map(move |y| [x, 64 + y, 0])).filter(|pos| *pos != [1, 65, 0]).collect();
        let (placement, shape) = screen(&ring, Side::North, Rotation::R0);
        assert_eq!(quads(&placement, &shape).len(), 8);
    }
}
