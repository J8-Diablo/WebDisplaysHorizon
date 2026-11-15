//! Fitting a picture into a screen of another shape: the larger side reaches the edge, the
//! other is centred, nothing is stretched ("contain"). A shared browser shown on several
//! screens is fitted like this on each.
//!
//! One function serves both the renderer (where the quad and its texture go) and the input
//! path (which browser pixel a click hit). Two copies of this arithmetic would agree for a
//! while and then drift, and clicks would land beside the cursor.
//!
//! Coordinates are over the screen's drawn area: 0 at one edge, 1 at the other.

/// What the bezel hides on each edge of a screen, in blocks: two pixels of a block's 16.
pub const BEZEL: f32 = 2.0 / 16.0;

/// Where a picture sits in a screen's drawn area.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    /// The picture's corner.
    pub offset: [f32; 2],
    /// The picture's size.
    pub size: [f32; 2],
}

impl Fit {
    /// The picture covers the screen: no bars.
    pub const FULL: Fit = Fit { offset: [0.0, 0.0], size: [1.0, 1.0] };

    /// Fits a picture of `content` aspect (width / height) in a screen of `screen` aspect.
    pub fn contain(screen: f32, content: f32) -> Fit {
        // Written so that NaN also falls through to FULL.
        if !(screen > 0.0) || !(content > 0.0) {
            return Fit::FULL;
        }
        let size = if content >= screen {
            // Wider than the screen: bars above and below.
            [1.0, screen / content]
        } else {
            // Taller: bars left and right.
            [content / screen, 1.0]
        };
        Fit { offset: [(1.0 - size[0]) * 0.5, (1.0 - size[1]) * 0.5], size }
    }

    /// The fit of a `content` picture (pixels) in a screen of `blocks`, its sides swapped
    /// when the screen is turned a quarter (`vertical`).
    pub fn for_screen(blocks: [u32; 2], vertical: bool, content: [u32; 2]) -> Fit {
        let [half_width, half_height] = drawn_half_extents(blocks, vertical);
        if content[0] == 0 || content[1] == 0 || !(half_width > 0.0) || !(half_height > 0.0) {
            return Fit::FULL;
        }
        Fit::contain(half_width / half_height, content[0] as f32 / content[1] as f32)
    }

    pub fn is_full(&self) -> bool {
        self.size[0] >= 1.0 && self.size[1] >= 1.0
    }

    /// Whether a point of the screen is on the picture, not on a bar.
    pub fn contains(&self, point: [f32; 2]) -> bool {
        (0..2).all(|i| point[i] >= self.offset[i] && point[i] <= self.offset[i] + self.size[i])
    }

    /// A point of the screen as a point of the picture even off the picture: below 0 or
    /// above 1 on a bar. What the renderer gives each corner of a quad; it paints what
    /// falls outside the picture black.
    pub fn to_content_or_bar(&self, point: [f32; 2]) -> [f32; 2] {
        [0, 1].map(|i| (point[i] - self.offset[i]) / self.size[i].max(1e-6))
    }

    /// A point of the screen as a point of the picture, both measured the same way round.
    /// None on a bar: a click there is nobody's.
    pub fn to_content(&self, point: [f32; 2]) -> Option<[f32; 2]> {
        self.contains(point).then(|| [0, 1].map(|i| (point[i] - self.offset[i]) / self.size[i]))
    }
}

/// Half the drawn width and height of a screen of `blocks`, in blocks: the bezel is off
/// each edge, and a quarter turn swaps the two. What the renderer builds its quad from.
pub fn drawn_half_extents(blocks: [u32; 2], vertical: bool) -> [f32; 2] {
    let half = blocks.map(|side| side as f32 * 0.5 - BEZEL);
    if vertical { [half[1], half[0]] } else { half }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn a_wider_picture_gets_bars_above_and_below() {
        let fit = Fit::contain(1.0, 2.0);
        assert!(close(fit.size[0], 1.0) && close(fit.size[1], 0.5));
        assert!(close(fit.offset[0], 0.0) && close(fit.offset[1], 0.25));
        assert!(!fit.is_full());
    }

    #[test]
    fn a_taller_picture_gets_bars_left_and_right() {
        let fit = Fit::contain(2.0, 1.0);
        assert!(close(fit.size[0], 0.5) && close(fit.size[1], 1.0));
        assert!(close(fit.offset[0], 0.25) && close(fit.offset[1], 0.0));
    }

    #[test]
    fn the_same_shape_and_nonsense_fill_the_screen() {
        assert!(Fit::contain(16.0 / 9.0, 16.0 / 9.0).is_full());
        for nonsense in [0.0, -1.0, f32::NAN] {
            assert_eq!(Fit::contain(nonsense, 1.0), Fit::FULL);
            assert_eq!(Fit::contain(1.0, nonsense), Fit::FULL);
        }
        assert_eq!(Fit::for_screen([4, 3], false, [0, 1080]), Fit::FULL);
        assert_eq!(Fit::for_screen([0, 3], false, [1920, 1080]), Fit::FULL);
    }

    #[test]
    fn a_click_on_a_bar_is_nobodys_and_one_on_the_picture_maps_to_it() {
        let fit = Fit::contain(1.0, 2.0);
        assert_eq!(fit.to_content([0.5, 0.1]), None, "on the top bar");
        assert_eq!(fit.to_content([0.5, 0.9]), None, "on the bottom bar");
        let centre = fit.to_content([0.5, 0.5]).unwrap();
        assert!(close(centre[0], 0.5) && close(centre[1], 0.5));
        let corner = fit.to_content([1.0, 0.75]).unwrap();
        assert!(close(corner[0], 1.0) && close(corner[1], 1.0));
    }

    #[test]
    fn the_bezel_is_off_each_edge_and_a_quarter_turn_swaps_the_sides() {
        let flat = drawn_half_extents([4, 2], false);
        assert!(close(flat[0], 2.0 - BEZEL) && close(flat[1], 1.0 - BEZEL));
        assert_eq!(drawn_half_extents([4, 2], true), [flat[1], flat[0]]);
        // A 4x2 screen is a little wider than 2:1 once the bezel is off, so a 2:1 picture
        // gets thin bars at its sides; turned a quarter it gets wide ones above and below.
        let fit = Fit::for_screen([4, 2], false, [2000, 1000]);
        assert!(fit.size[0] < 1.0 && close(fit.size[1], 1.0));
        let turned = Fit::for_screen([4, 2], true, [2000, 1000]);
        assert!(close(turned.size[0], 1.0) && turned.size[1] < 0.3);
    }
}
