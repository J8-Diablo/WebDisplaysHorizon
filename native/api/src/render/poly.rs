//! A traced screen as quads: a thin sheet standing along the outline the player traced,
//! the page on its front, a dark panel on its back and round its edges.
//!
//! The sheet is cut into strips where its outline or its curve changes (`Surface::strips`);
//! each strip is a flat four-sided piece. The renderer draws a quad from its front only,
//! and recognises the front by the order of the corners, so every quad here is put in the
//! order that faces the way it should (`facing`).
//!
//! Also here: where a look lands on the sheet (`aim`), and where its sound and its light
//! come from. Plain arithmetic on `model::poly`, tested without the game.
use super::geometry::Quad;
use super::glow::{CELLS, Glow, cell_centre};
use crate::model::letterbox::Fit;
use crate::model::poly::{Surface, THICKNESS};

/// How far in front of the panel the page is drawn, in blocks.
const LIFT: f32 = 0.004;

type Point = [f64; 3];

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: Point, b: Point) -> Point {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// A quad over `corners` (any order round the piece) seen from the side `normal` points
/// to. The renderer's front is the side from which the corners run top-left, top-right,
/// bottom-right, bottom-left: when these run the other way they are taken backwards.
fn facing(mut corners: [Point; 4], mut uv: [[f32; 2]; 4], normal: Point, page: u32) -> Quad {
    // For a quad seen from its front, this cross product points away from the viewer.
    let away = cross(sub(corners[1], corners[0]), sub(corners[3], corners[0]));
    if dot(away, normal) > 0.0 {
        corners.swap(0, 1);
        corners.swap(2, 3);
        uv.swap(0, 1);
        uv.swap(2, 3);
    }
    Quad { corners, uv, page }
}

/// A point of the sheet `out` blocks off its middle plane, on the side its seam's offset
/// points to.
fn point(surface: &Surface, u: f32, v: f32, out: f32) -> Point {
    let [x, z] = surface.xz_at(u);
    let [ox, oz] = surface.offset_at(u, out);
    [f64::from(x + ox), f64::from(v), f64::from(z + oz)]
}

/// Where a picture of `picture` pixels sits on the sheet when it keeps its own shape (a
/// shared browser's).
pub fn fit(surface: &Surface, picture: [u32; 2]) -> Fit {
    Fit::contain(surface.width() / surface.height().max(1e-6), picture[0] as f32 / picture[1].max(1) as f32)
}

/// The quads of a traced screen: the page (`page`) on its front, `panel` (a plain dark
/// page) on its back and its edges. With `fit`, the picture keeps its own shape and the
/// rest of the sheet is bars.
pub fn quads(surface: &Surface, page: u32, panel: u32, fit: Option<&Fit>) -> Vec<Quad> {
    let half = THICKNESS / 2.0;
    let front = (half + LIFT) * surface.facing();
    let back = -half * surface.facing();
    let plain = [[0.5, 0.5]; 4];
    let mut all = Vec::new();
    for strip in surface.strips() {
        let [a, b] = strip.u;
        let [nx, nz] = surface.normal_at((a + b) / 2.0);
        let ahead: Point = [f64::from(nx * surface.facing()), 0.0, f64::from(nz * surface.facing())];
        let behind = ahead.map(|part| -part);
        // The four corners of the strip: bottom and top at each seam.
        let corner = |out: f32| [point(surface, a, strip.left[1], out), point(surface, b, strip.right[1], out), point(surface, b, strip.right[0], out), point(surface, a, strip.left[0], out)];
        let on_page = |u: f32, v: f32| match fit {
            Some(fit) => fit.to_content_or_bar(surface.page_uv(u, v)),
            None => surface.page_uv(u, v),
        };
        let uv = [on_page(a, strip.left[1]), on_page(b, strip.right[1]), on_page(b, strip.right[0]), on_page(a, strip.left[0])];
        all.push(facing(corner(back), plain, behind, panel));
        // The edges: top and bottom, and an end where the sheet ends.
        let (near, far) = (corner(front), corner(back));
        all.push(facing([near[0], near[1], far[1], far[0]], plain, [0.0, 1.0, 0.0], panel));
        all.push(facing([near[3], near[2], far[2], far[3]], plain, [0.0, -1.0, 0.0], panel));
        let along: Point = sub(near[1], near[0]);
        if strip.caps[0] {
            all.push(facing([near[0], far[0], far[3], near[3]], plain, along.map(|part| -part), panel));
        }
        if strip.caps[1] {
            all.push(facing([near[1], far[1], far[2], near[2]], plain, along, panel));
        }
        // The page last: it is drawn over the panel's edges where they meet.
        all.push(facing(corner(front), uv, ahead, page));
    }
    all
}

/// A thin bar from `a` to `b`, `thick` across, seen from every side: two flat pieces
/// crossed, each drawn from both of its sides.
fn bar(a: Point, b: Point, thick: f64, page: u32, all: &mut Vec<Quad>) {
    let along = sub(b, a);
    // Two directions across the bar: away from whichever axis it runs along most.
    let other: Point = if along[1].abs() > along[0].abs().max(along[2].abs()) { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let first = cross(along, other);
    let second = cross(along, first);
    for across in [first, second] {
        let long = dot(across, across).sqrt();
        if long < 1e-9 {
            continue;
        }
        let half = across.map(|part| part / long * thick / 2.0);
        let corners = [[a[0] - half[0], a[1] - half[1], a[2] - half[2]], [b[0] - half[0], b[1] - half[1], b[2] - half[2]], [b[0] + half[0], b[1] + half[1], b[2] + half[2]], [a[0] + half[0], a[1] + half[1], a[2] + half[2]]];
        let uv = [[0.5, 0.5]; 4];
        all.push(Quad { corners, uv, page });
        all.push(Quad { corners: [corners[1], corners[0], corners[3], corners[2]], uv, page });
    }
}

/// The outline a player is tracing, to draw: a bar between each corner and the next, and a
/// small cross at each corner so that the first one, alone, shows too.
pub fn trace(points: &[[i32; 3]], page: u32) -> Vec<Quad> {
    let corners: Vec<Point> = points.iter().map(|point| point.map(f64::from)).collect();
    let mut all = Vec::new();
    for corner in &corners {
        for axis in 0..3 {
            let (mut low, mut high) = (*corner, *corner);
            low[axis] -= 0.2;
            high[axis] += 0.2;
            bar(low, high, 0.08, page, &mut all);
        }
    }
    for pair in corners.windows(2) {
        bar(pair[0], pair[1], 0.05, page, &mut all);
    }
    all
}

/// The outline of a screen already built, to draw for a player holding the pen: its
/// corners as `trace` marks them, and thicker bars all the way round, the last corner
/// joined to the first.
pub fn built(points: &[[i32; 3]], page: u32) -> Vec<Quad> {
    let corners: Vec<Point> = points.iter().map(|point| point.map(f64::from)).collect();
    let mut all = trace(points, page);
    if corners.len() < 3 {
        return all;
    }
    for (index, corner) in corners.iter().enumerate() {
        bar(*corner, corners[(index + 1) % corners.len()], 0.09, page, &mut all);
    }
    all
}

/// Where a look lands on the sheet: `eye` is where it starts, `through` a point further
/// along it near the sheet (where the game says the look hit one of the screen's blocks).
/// Gives the point of the sheet as (u, v), when the look crosses it within a couple of
/// blocks of `through` and where the sheet is filled.
pub fn aim(surface: &Surface, eye: Point, through: Point) -> Option<[f32; 2]> {
    let way = sub(through, eye);
    let long = dot(way, way).sqrt();
    if long < 1e-6 {
        return None;
    }
    let unit = way.map(|part| part / long);
    let at = |t: f64| -> Point { [eye[0] + unit[0] * t, eye[1] + unit[1] * t, eye[2] + unit[2] * t] };
    // Which side of the sheet a point is on: its distance along the sheet's normal there.
    let side = |p: Point| -> f64 {
        let u = surface.nearest_u(p[0], p[2]);
        let [x, z] = surface.xz_at(u);
        let [nx, nz] = surface.normal_at(u);
        (p[0] - f64::from(x)) * f64::from(nx) + (p[2] - f64::from(z)) * f64::from(nz)
    };
    const STEPS: usize = 48;
    let (from, to) = ((long - 2.0).max(0.0), long + 2.0);
    let step = (to - from) / STEPS as f64;
    let mut last = side(at(from));
    for index in 1..=STEPS {
        let t = from + step * index as f64;
        let now = side(at(t));
        if (last <= 0.0) != (now <= 0.0) {
            // Crossed between the two samples: where, in proportion.
            let part = last.abs() / (last.abs() + now.abs()).max(1e-9);
            let hit = at(t - step + step * part);
            if let Some(found) = surface.from_world(hit[0], hit[1], hit[2]) {
                return Some(found);
            }
        }
        last = now;
    }
    None
}

/// The two points a traced screen's sound comes from: the page's left channel, then its
/// right one, each a quarter in from its edge, half-way up, just in front of the sheet.
pub fn sound_points(surface: &Surface) -> [Point; 2] {
    let middle = (surface.min_v() + surface.max_v()) / 2.0;
    let at = |share: f32| point(surface, surface.min_u() + surface.width() * share, middle, 0.4 * surface.facing());
    let (low, high) = (at(0.25), at(0.75));
    // Which end is the picture's left depends on which way the sheet faces.
    let low_is_left = surface.page_uv(surface.min_u() + surface.width() * 0.25, middle)[0] < 0.5;
    if low_is_left { [low, high] } else { [high, low] }
}

/// The lights a traced screen throws: one for each cell of its picture, where that cell is
/// on the sheet, shining the way the sheet faces there.
pub fn glows(surface: &Surface, page: u32) -> Vec<Glow> {
    let reach = 3.0 + 1.5 * surface.width().max(surface.height());
    // Where the picture's left edge is along the sheet: found by asking, as for the sound.
    let middle = (surface.min_v() + surface.max_v()) / 2.0;
    let mirrored = surface.page_uv(surface.min_u() + surface.width() * 0.25, middle)[0] > 0.5;
    (0..CELLS)
        .map(|cell| {
            let [across, down] = cell_centre(cell);
            let share = if mirrored { 1.0 - across } else { across };
            let u = surface.min_u() + surface.width() * share;
            let v = surface.max_v() - surface.height() * down;
            let [nx, nz] = surface.normal_at(u);
            Glow { page, cell, position: point(surface, u, v, 0.2 * surface.facing()), normal: [nx * surface.facing(), 0.0, nz * surface.facing()], reach }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::poly::Outline;

    /// A flat sheet six blocks wide and four high, standing along x at z = 0.
    fn wall() -> Surface {
        Surface::of(&Outline::from_points([[0, 64, 0], [6, 64, 0], [6, 68, 0], [0, 68, 0]])).expect("a wall is a surface")
    }

    fn front_of(quad: &Quad) -> Point {
        // The side a quad is seen from: against the cross product of its first two edges.
        cross(sub(quad.corners[1], quad.corners[0]), sub(quad.corners[3], quad.corners[0])).map(|part| -part)
    }

    #[test]
    fn the_page_faces_the_front_and_the_panel_the_back() {
        let surface = wall();
        let all = quads(&surface, 7, 99, None);
        let pages: Vec<&Quad> = all.iter().filter(|quad| quad.page == 7).collect();
        assert!(!pages.is_empty());
        let [nx, nz] = surface.normal_at(3.0);
        let ahead = [f64::from(nx * surface.facing()), 0.0, f64::from(nz * surface.facing())];
        for page in &pages {
            assert!(dot(front_of(page), ahead) > 0.0, "a page quad is seen from the front");
        }
        let backs = all.iter().filter(|quad| quad.page == 99 && dot(front_of(quad), ahead) < -0.5 * dot(front_of(quad), front_of(quad)).sqrt()).count();
        assert!(backs >= pages.len(), "each piece has a back");
        // Together the pages show the whole picture once.
        let (mut least, mut most) = ([f32::MAX; 2], [f32::MIN; 2]);
        for page in &pages {
            for uv in page.uv {
                for i in 0..2 {
                    least[i] = least[i].min(uv[i]);
                    most[i] = most[i].max(uv[i]);
                }
            }
        }
        assert!(least[0].abs() < 1e-3 && least[1].abs() < 1e-3 && (most[0] - 1.0).abs() < 1e-3 && (most[1] - 1.0).abs() < 1e-3, "{least:?} {most:?}");
    }

    #[test]
    fn the_page_is_in_front_of_the_panel_by_the_sheets_thickness() {
        let surface = wall();
        let all = quads(&surface, 7, 99, None);
        let page_z = all.iter().find(|quad| quad.page == 7).unwrap().corners[0][2];
        let [_, nz] = surface.normal_at(3.0);
        let expected = f64::from((THICKNESS / 2.0 + LIFT) * surface.facing() * nz);
        assert!((page_z - expected).abs() < 1e-5, "{page_z} against {expected}");
    }

    #[test]
    fn a_look_at_the_sheet_lands_where_it_points() {
        let surface = wall();
        let [_, nz] = surface.normal_at(3.0);
        // Stand four blocks in front of the sheet, look at a point of it.
        let front = f64::from(nz * surface.facing()).signum();
        let eye = [2.0, 66.5, 4.0 * front];
        let target = [4.5, 65.25, 0.0];
        // The game reports the hit on the face of the block the sheet stands in: half a block early.
        let through = [eye[0] + (target[0] - eye[0]) * 0.88, eye[1] + (target[1] - eye[1]) * 0.88, eye[2] + (target[2] - eye[2]) * 0.88];
        let [u, v] = aim(&surface, eye, through).expect("the look crosses the sheet");
        let landed = surface.to_world(u, v);
        assert!((f64::from(landed[0]) - 4.5).abs() < 0.03 && (f64::from(landed[1]) - 65.25).abs() < 0.03, "{landed:?}");
        // A look that passes over the sheet lands nowhere.
        assert_eq!(aim(&surface, eye, [4.5, 72.0, 0.4 * front]), None);
    }

    #[test]
    fn a_traced_outline_shows_its_corners_and_what_joins_them() {
        assert!(trace(&[], 3).is_empty());
        let one = trace(&[[0, 64, 0]], 3);
        assert_eq!(one.len(), 3 * 4, "a cross of three bars, each two pieces seen from both sides");
        let two = trace(&[[0, 64, 0], [5, 64, 0]], 3);
        assert_eq!(two.len(), 2 * 12 + 4);
        assert!(two.iter().all(|quad| quad.page == 3));
        // The bar between the two corners spans them.
        let (mut least, mut most) = (f64::MAX, f64::MIN);
        for quad in &two[24..] {
            for corner in quad.corners {
                least = least.min(corner[0]);
                most = most.max(corner[0]);
            }
        }
        assert_eq!((least, most), (0.0, 5.0));
        // A bar straight up is as visible as one lying flat.
        let up = trace(&[[0, 64, 0], [0, 70, 0]], 3);
        assert!(up[24..].iter().all(|quad| quad.corners.iter().any(|corner| corner[1] == 70.0)));
    }

    #[test]
    fn sound_and_light_come_from_in_front_of_the_sheet() {
        let surface = wall();
        let [_, nz] = surface.normal_at(3.0);
        let front = f64::from(nz * surface.facing()).signum();
        let [left, right] = sound_points(&surface);
        assert!(left[2] * front > 0.2 && right[2] * front > 0.2, "{left:?} {right:?}");
        assert!((left[0] - right[0]).abs() > 2.0, "a quarter in from each end");
        assert!((left[1] - 66.0).abs() < 1e-4);
        let lights = glows(&surface, 7);
        assert_eq!(lights.len(), CELLS);
        // Cell 0 is the picture's top-left: on the same end of the sheet as the left channel.
        assert!((lights[0].position[0] - left[0]).abs() < (lights[0].position[0] - right[0]).abs());
        assert!(lights[0].position[1] > lights[CELLS - 1].position[1], "the top cells are higher");
        assert!(lights.iter().all(|light| f64::from(light.normal[2]) * front > 0.9));
    }

    #[test]
    fn a_built_outline_is_drawn_closed() {
        let square = [[0, 64, 0], [4, 64, 0], [4, 67, 0], [0, 67, 0]];
        // A bar is two crossed pieces seen from both sides: four quads.
        let (open, closed) = (trace(&square, 7), built(&square, 7));
        assert_eq!(closed.len(), open.len() + 4 * square.len(), "one more bar a side, the last one back to the first corner");
        assert!(closed.iter().all(|quad| quad.page == 7));
        // The closing side runs from the last corner to the first: up the x = 0 edge.
        let closing = &closed[closed.len() - 4..];
        assert!(closing.iter().all(|quad| quad.corners.iter().all(|corner| corner[0].abs() < 0.1 && corner[2].abs() < 0.1)), "{closing:?}");
        // Fewer than three corners enclose nothing: they are drawn as a trace.
        assert_eq!(built(&square[..2], 7).len(), trace(&square[..2], 7).len());
        assert!(built(&[], 7).is_empty());
    }
}
