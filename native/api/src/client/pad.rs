//! The minePad: a browser in the hand. Where it is drawn on the picture, held and opened,
//! and which pixel of its page a point of the window is.
//!
//! The pad is not in the world: it is drawn flat over the picture, after the world, where a
//! hand would hold it, and larger in the middle once opened. Its page is the player's own
//! (no other player sees it), so nothing of it goes through the server.
//!
//! Places are in units of half the picture's height, from its middle: y from -1 (bottom)
//! to 1 (top), x from minus the picture's aspect to plus it. The renderer squeezes x by the
//! aspect, so a pad keeps its shape on any window. Plain arithmetic, tested without the game.
use crate::render::geometry::Quad;

/// The page's size in pixels: a small tablet's, as wide as high times 16/10.
pub const PAGE: [u32; 2] = [1024, 640];
const ASPECT: f32 = PAGE[0] as f32 / PAGE[1] as f32;
/// The border of the pad round its page, in the same units as its place.
const FRAME: f32 = 0.03;

/// A rectangle on the picture: its middle, and half its width and height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub middle: [f32; 2],
    pub half: [f32; 2],
}

/// Where the page is when the pad is held: low on the right, as in a right hand.
pub fn held() -> Place {
    let half_height = 0.28;
    Place { middle: [0.92, -0.62], half: [half_height * ASPECT, half_height] }
}

/// Where the page is when the pad is opened: the middle of the picture, most of its height.
pub fn opened() -> Place {
    let half_height = 0.72;
    Place { middle: [0.0, -0.04], half: [half_height * ASPECT, half_height] }
}

/// A rectangle from (`left`, `bottom`) to (`right`, `top`) as two quads, one each way
/// round: whichever the renderer takes for the front, it is seen.
fn rect(left: f32, bottom: f32, right: f32, top: f32, page: u32, whole: bool) -> [Quad; 2] {
    let (l, b, r, t) = (f64::from(left), f64::from(bottom), f64::from(right), f64::from(top));
    // Top-left, top-right, bottom-right, bottom-left, flat at the front of the picture.
    let corners = [[l, t, 0.0], [r, t, 0.0], [r, b, 0.0], [l, b, 0.0]];
    let uv = if whole { [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] } else { [[0.5, 0.5]; 4] };
    [Quad { corners, uv, page }, Quad { corners: [corners[1], corners[0], corners[3], corners[2]], uv: [uv[1], uv[0], uv[3], uv[2]], page }]
}

/// The pad as quads over the picture: its page, and its dark body as a frame round it.
/// The frame's four pieces do not go under the page: the renderer draws page by page, in
/// no order a pad could count on.
pub fn quads(place: Place, page: u32, body: u32) -> Vec<Quad> {
    let (left, right) = (place.middle[0] - place.half[0], place.middle[0] + place.half[0]);
    let (bottom, top) = (place.middle[1] - place.half[1], place.middle[1] + place.half[1]);
    let mut all = Vec::with_capacity(10);
    all.extend(rect(left - FRAME, top, right + FRAME, top + FRAME, body, false));
    all.extend(rect(left - FRAME, bottom - FRAME, right + FRAME, bottom, body, false));
    all.extend(rect(left - FRAME, bottom, left, top, body, false));
    all.extend(rect(right, bottom, right + FRAME, top, body, false));
    all.extend(rect(left, bottom, right, top, page, true));
    all
}

/// The pixel of the page under a point of the game's window. `at` and `window` (x, y,
/// width, height) are in GUI pixels, y downwards. None off the page.
pub fn to_pixel(place: Place, at: [f32; 2], window: [f32; 4]) -> Option<[u32; 2]> {
    let unit = window[3] / 2.0;
    if unit <= 0.0 {
        return None;
    }
    // The point in the pad's units, from the picture's middle, y upwards.
    let x = (at[0] - window[0] - window[2] / 2.0) / unit;
    let y = (window[1] + window[3] / 2.0 - at[1]) / unit;
    let across = (x - (place.middle[0] - place.half[0])) / (2.0 * place.half[0]);
    let down = ((place.middle[1] + place.half[1]) - y) / (2.0 * place.half[1]);
    if !(0.0..1.0).contains(&across) || !(0.0..1.0).contains(&down) {
        return None;
    }
    Some([(across * PAGE[0] as f32) as u32, (down * PAGE[1] as f32) as u32])
}

/// The rectangle of the window (GUI pixels) the opened page covers: what the window's own
/// widgets are laid out round.
pub fn window_rect(place: Place, window: [f32; 4]) -> [f32; 4] {
    let unit = window[3] / 2.0;
    let left = window[0] + window[2] / 2.0 + (place.middle[0] - place.half[0]) * unit;
    let top = window[1] + window[3] / 2.0 - (place.middle[1] + place.half[1]) * unit;
    [left, top, 2.0 * place.half[0] * unit, 2.0 * place.half[1] * unit]
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: [f32; 4] = [0.0, 0.0, 480.0, 270.0];

    #[test]
    fn the_page_keeps_its_shape_wherever_the_pad_is() {
        for place in [held(), opened()] {
            assert!((place.half[0] / place.half[1] - ASPECT).abs() < 1e-5);
            assert!(place.middle[1] - place.half[1] - FRAME > -1.0 && place.middle[1] + place.half[1] + FRAME < 1.0, "inside the picture's height");
        }
        assert!(opened().half[1] > 2.0 * held().half[1], "opened, it is much larger");
    }

    #[test]
    fn the_pad_is_its_page_in_a_dark_frame() {
        let all = quads(opened(), 5, 99);
        let (frame, pages): (Vec<&Quad>, Vec<&Quad>) = all.iter().partition(|quad| quad.page == 99);
        assert_eq!((frame.len(), pages.len()), (8, 2), "four pieces of frame and the page, each seen both ways round");
        let page = pages[0];
        assert_eq!(page.uv, [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        assert!(page.corners[0][0] < page.corners[1][0] && page.corners[0][1] > page.corners[3][1], "top-left first");
        // No piece of the frame goes under the page.
        let (left, right, bottom, top) = (page.corners[0][0], page.corners[1][0], page.corners[3][1], page.corners[0][1]);
        for piece in frame {
            let xs: Vec<f64> = piece.corners.iter().map(|corner| corner[0]).collect();
            let ys: Vec<f64> = piece.corners.iter().map(|corner| corner[1]).collect();
            let (l, r) = (xs.iter().cloned().fold(f64::MAX, f64::min), xs.iter().cloned().fold(f64::MIN, f64::max));
            let (b, t) = (ys.iter().cloned().fold(f64::MAX, f64::min), ys.iter().cloned().fold(f64::MIN, f64::max));
            assert!(r <= left + 1e-6 || l >= right - 1e-6 || t <= bottom + 1e-6 || b >= top - 1e-6, "{piece:?}");
        }
    }

    #[test]
    fn a_point_of_the_window_is_a_pixel_of_the_page() {
        let place = opened();
        let rect = window_rect(place, WINDOW);
        assert!(rect[0] > 0.0 && rect[0] + rect[2] < WINDOW[2] && rect[1] > 0.0 && rect[1] + rect[3] < WINDOW[3], "{rect:?}");
        assert_eq!(to_pixel(place, [rect[0] + 0.01, rect[1] + 0.01], WINDOW), Some([0, 0]));
        let middle = to_pixel(place, [rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0], WINDOW).unwrap();
        assert!(middle[0].abs_diff(PAGE[0] / 2) <= 1 && middle[1].abs_diff(PAGE[1] / 2) <= 1, "{middle:?}");
        let last = to_pixel(place, [rect[0] + rect[2] - 0.01, rect[1] + rect[3] - 0.01], WINDOW).unwrap();
        assert_eq!(last, [PAGE[0] - 1, PAGE[1] - 1]);
        assert_eq!(to_pixel(place, [rect[0] - 1.0, rect[1] + 5.0], WINDOW), None, "beside the page");
        assert_eq!(to_pixel(place, [rect[0] + 5.0, rect[1] + rect[3] + 1.0], WINDOW), None, "under it");
    }

    #[test]
    fn the_same_point_is_the_same_pixel_on_a_window_shifted_or_scaled() {
        let place = opened();
        let big = [10.0, 20.0, 960.0, 540.0];
        let rect = window_rect(place, big);
        let a = to_pixel(place, [rect[0] + rect[2] * 0.25, rect[1] + rect[3] * 0.75], big).unwrap();
        let small = window_rect(place, WINDOW);
        let b = to_pixel(place, [small[0] + small[2] * 0.25, small[1] + small[3] * 0.75], WINDOW).unwrap();
        assert!(a[0].abs_diff(b[0]) <= 1 && a[1].abs_diff(b[1]) <= 1, "{a:?} {b:?}");
    }
}
