//! What the renderer draws, before any of it reaches the GPU: plain numbers, so that it
//! is tested without the game (`tools/dev-logic`).
use crate::proto::frame;

/// A flat piece of a browser's picture in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    /// Its corners in world coordinates: top-left, top-right, bottom-right, bottom-left,
    /// as the picture is seen from its front.
    pub corners: [[f64; 3]; 4],
    /// Where each corner is in the picture, 0 to 1 across and down.
    pub uv: [[f32; 2]; 4],
    /// The browser whose picture it shows.
    pub page: u32,
}

impl Quad {
    /// The whole picture of `page` on these corners.
    pub fn whole(corners: [[f64; 3]; 4], page: u32) -> Self {
        Self { corners, uv: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], page }
    }
}

/// Bytes a vertex takes: position (relative to the camera), then where in the picture.
pub const VERTEX: usize = 20;
/// Vertices a quad takes: two triangles.
pub const QUAD_VERTICES: u32 = 6;

/// The quads of one page in the vertex buffer: which page, and how many quads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub page: u32,
    pub quads: u32,
}

/// The vertices of `quads` seen from `camera`, grouped by page so that each page is one
/// draw: positions relative to the camera (the world is too large for `f32` positions of
/// its own), two triangles a quad. The runs say which vertices are whose, in order.
pub fn vertices(quads: &[Quad], camera: [f64; 3]) -> (Vec<u8>, Vec<Run>) {
    let mut order: Vec<&Quad> = quads.iter().collect();
    order.sort_by_key(|quad| quad.page);
    let mut bytes = Vec::with_capacity(quads.len() * QUAD_VERTICES as usize * VERTEX);
    let mut runs: Vec<Run> = Vec::new();
    for quad in order {
        match runs.last_mut() {
            Some(run) if run.page == quad.page => run.quads += 1,
            _ => runs.push(Run { page: quad.page, quads: 1 }),
        }
        for corner in [0, 1, 2, 0, 2, 3] {
            for axis in 0..3 {
                bytes.extend(((quad.corners[corner][axis] - camera[axis]) as f32).to_le_bytes());
            }
            bytes.extend(quad.uv[corner][0].to_le_bytes());
            bytes.extend(quad.uv[corner][1].to_le_bytes());
        }
    }
    (bytes, runs)
}

/// The test card's size.
pub const CARD: [u32; 2] = [256, 144];

/// A picture that shows which way up and round a quad is: a grey chequerboard with a red
/// top-left corner, green top-right, blue bottom-left and white bottom-right. BGRA.
pub fn test_card() -> Vec<u8> {
    let [width, height] = CARD;
    let mut out = Vec::with_capacity((width * height) as usize * frame::PIXEL);
    for y in 0..height {
        for x in 0..width {
            let (left, top) = (x < 48, y < 48);
            let (right, bottom) = (x >= width - 48, y >= height - 48);
            let shade = if (x / 16 + y / 16) % 2 == 0 { 70 } else { 150 };
            let bgra = match (left, right, top, bottom) {
                (true, _, true, _) => [0, 0, 255, 255],
                (_, true, true, _) => [0, 255, 0, 255],
                (true, _, _, true) => [255, 0, 0, 255],
                (_, true, _, true) => [255, 255, 255, 255],
                _ => [shade, shade, shade, 255],
            };
            out.extend(bgra);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(page: u32, x: f64) -> Quad {
        Quad::whole([[x, 5.0, 3.0], [x + 2.0, 5.0, 3.0], [x + 2.0, 4.0, 3.0], [x, 4.0, 3.0]], page)
    }

    fn vertex(bytes: &[u8], index: usize) -> [f32; 5] {
        [0, 4, 8, 12, 16].map(|field| {
            let at = index * VERTEX + field;
            f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
        })
    }

    #[test]
    fn a_quad_is_two_triangles_relative_to_the_camera() {
        let (bytes, runs) = vertices(&[quad(7, 10.0)], [10.0, 4.0, 0.0]);
        assert_eq!(bytes.len(), 6 * VERTEX);
        assert_eq!(runs, [Run { page: 7, quads: 1 }]);
        assert_eq!(vertex(&bytes, 0), [0.0, 1.0, 3.0, 0.0, 0.0], "top-left, the picture's first texel");
        assert_eq!(vertex(&bytes, 1), [2.0, 1.0, 3.0, 1.0, 0.0], "top-right");
        assert_eq!(vertex(&bytes, 2), [2.0, 0.0, 3.0, 1.0, 1.0], "bottom-right");
        assert_eq!(vertex(&bytes, 3), vertex(&bytes, 0), "the second triangle starts where the first did");
        assert_eq!(vertex(&bytes, 5), [0.0, 0.0, 3.0, 0.0, 1.0], "bottom-left");
    }

    #[test]
    fn a_quad_can_show_a_part_of_the_picture() {
        let mut part = quad(1, 0.0);
        part.uv = [[0.25, 0.5], [0.5, 0.5], [0.5, 1.0], [0.25, 1.0]];
        let (bytes, _) = vertices(&[part], [0.0; 3]);
        assert_eq!(&vertex(&bytes, 0)[3..], [0.25, 0.5]);
        assert_eq!(&vertex(&bytes, 2)[3..], [0.5, 1.0]);
    }

    #[test]
    fn quads_are_grouped_by_page_whatever_order_they_came_in() {
        let (bytes, runs) = vertices(&[quad(2, 0.0), quad(1, 10.0), quad(2, 20.0), quad(1, 30.0), quad(3, 40.0)], [0.0; 3]);
        assert_eq!(runs, [Run { page: 1, quads: 2 }, Run { page: 2, quads: 2 }, Run { page: 3, quads: 1 }]);
        // Page 1's quads come first, in the order they were given.
        assert_eq!(vertex(&bytes, 0)[0], 10.0);
        assert_eq!(vertex(&bytes, 6)[0], 30.0);
        assert_eq!(vertex(&bytes, 12)[0], 0.0);
        assert!(vertices(&[], [0.0; 3]).1.is_empty());
    }

    #[test]
    fn far_from_the_origin_positions_keep_their_precision() {
        // Thirty million blocks out, an f32 world position would be off by two blocks.
        let far = 30_000_000.0;
        let (bytes, _) = vertices(&[Quad::whole([[far + 0.25, 64.0, far]; 4], 0)], [far, 64.0, far]);
        assert_eq!(vertex(&bytes, 0)[0], 0.25);
    }

    #[test]
    fn the_test_card_marks_its_corners() {
        let card = test_card();
        assert_eq!(card.len(), (CARD[0] * CARD[1]) as usize * frame::PIXEL);
        let pixel = |x: u32, y: u32| {
            let at = (y * CARD[0] + x) as usize * frame::PIXEL;
            [card[at], card[at + 1], card[at + 2], card[at + 3]]
        };
        assert_eq!(pixel(0, 0), [0, 0, 255, 255], "red, as BGRA");
        assert_eq!(pixel(CARD[0] - 1, 0), [0, 255, 0, 255], "green");
        assert_eq!(pixel(0, CARD[1] - 1), [255, 0, 0, 255], "blue");
        assert_eq!(pixel(CARD[0] - 1, CARD[1] - 1), [255, 255, 255, 255], "white");
    }
}
