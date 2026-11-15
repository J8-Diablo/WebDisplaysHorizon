//! The light a screen throws on what is around it (the Java mod's "Ambilight"), before any
//! of it reaches the GPU: which colours, from where, how far.
//!
//! A screen's picture is cut into a few cells; each cell is a small light on the screen's
//! face, of the cell's average colour, shining out of the screen. The renderer lays those
//! lights over the frame (`render/ambilight.rs`), as many as there are: Horizon's built-in
//! coloured lights stop at eight a mod, which is two screens.
//!
//! Plain numbers, tested without the game.

/// The cells a picture is cut into, across and down.
pub const GRID: [usize; 2] = [2, 2];
/// Cells a picture has.
pub const CELLS: usize = GRID[0] * GRID[1];
/// The most lights laid over one frame; with more screens in view, the nearest win.
pub const MAX_LIGHTS: usize = 32;
/// Bytes one light takes in the shader's list: where and how far, facing, colour.
pub const LIGHT: usize = 48;
/// How much of the way to the picture's new colours the lights go each time a frame
/// arrives: a cut from a dark shot to a bright one washes over the room, it does not flash.
pub const FOLLOW: f32 = 0.25;

/// One cell's light, as the game places it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glow {
    /// The browser whose picture colours it.
    pub page: u32,
    /// Which cell of the picture: row by row from the top-left.
    pub cell: usize,
    /// Where it is in the world: on the screen, in the middle of its cell.
    pub position: [f64; 3],
    /// The way the screen faces.
    pub normal: [f32; 3],
    /// How far it shines, in blocks.
    pub reach: f32,
}

/// The middle of cell `cell` in the picture, 0 to 1 across and down.
pub fn cell_centre(cell: usize) -> [f32; 2] {
    let (column, row) = (cell % GRID[0], cell / GRID[0]);
    [(column as f32 + 0.5) / GRID[0] as f32, (row as f32 + 0.5) / GRID[1] as f32]
}

/// The average colour of each cell of a BGRA picture, red-green-blue from 0 to 1. Every
/// few pixels are looked at, not all: a light does not need more.
pub fn cell_colors(bgra: &[u8], size: [u32; 2]) -> [[f32; 3]; CELLS] {
    let mut colors = [[0f32; 3]; CELLS];
    let [width, height] = size.map(|side| side as usize);
    if width == 0 || height == 0 || bgra.len() < width * height * 4 {
        return colors;
    }
    // About sixteen samples a cell each way, whatever the picture's size.
    let step = [(width / (GRID[0] * 16)).max(1), (height / (GRID[1] * 16)).max(1)];
    for (cell, color) in colors.iter_mut().enumerate() {
        let (column, row) = (cell % GRID[0], cell / GRID[0]);
        let (left, right) = (column * width / GRID[0], (column + 1) * width / GRID[0]);
        let (top, bottom) = (row * height / GRID[1], (row + 1) * height / GRID[1]);
        let (mut sum, mut count) = ([0u64; 3], 0u64);
        for y in (top..bottom).step_by(step[1]) {
            for x in (left..right).step_by(step[0]) {
                let at = (y * width + x) * 4;
                sum[0] += u64::from(bgra[at + 2]);
                sum[1] += u64::from(bgra[at + 1]);
                sum[2] += u64::from(bgra[at]);
                count += 1;
            }
        }
        if count > 0 {
            *color = sum.map(|part| part as f32 / count as f32 / 255.0);
        }
    }
    colors
}

/// `shown` a step closer to `picture`.
pub fn follow(shown: &mut [[f32; 3]; CELLS], picture: &[[f32; 3]; CELLS]) {
    for (shown, picture) in shown.iter_mut().zip(picture) {
        for channel in 0..3 {
            shown[channel] += (picture[channel] - shown[channel]) * FOLLOW;
        }
    }
}

/// A colour as painted (gamma-encoded) as linear light, for a frame that is linear.
pub fn to_linear(color: [f32; 3]) -> [f32; 3] {
    color.map(|part| if part <= 0.04045 { part / 12.92 } else { ((part + 0.055) / 1.055).powf(2.4) })
}

/// The lights of a frame as the shader takes them: the nearest `MAX_LIGHTS` of `glows` to
/// the camera that have any colour, each `LIGHT` bytes (position relative to the camera
/// and reach, facing, colour). Gives the bytes and how many lights they hold.
pub fn pack(glows: &[Glow], color_of: impl Fn(&Glow) -> Option<[f32; 3]>, camera: [f64; 3]) -> (Vec<u8>, u32) {
    let mut lit: Vec<(f32, [f32; 3], &Glow, [f32; 3])> = glows
        .iter()
        .filter_map(|glow| {
            let color = color_of(glow)?;
            // A black cell throws no light: it takes no place in the list.
            if color.iter().all(|part| *part < 0.004) {
                return None;
            }
            let at = [0, 1, 2].map(|i| (glow.position[i] - camera[i]) as f32);
            let away = at.iter().map(|part| part * part).sum::<f32>();
            Some((away, at, glow, color))
        })
        .collect();
    lit.sort_by(|a, b| a.0.total_cmp(&b.0));
    lit.truncate(MAX_LIGHTS);
    let mut bytes = Vec::with_capacity(MAX_LIGHTS * LIGHT);
    for (_, at, glow, color) in &lit {
        for value in [at[0], at[1], at[2], glow.reach, glow.normal[0], glow.normal[1], glow.normal[2], 0.0, color[0], color[1], color[2], 0.0] {
            bytes.extend(value.to_le_bytes());
        }
    }
    let count = lit.len() as u32;
    bytes.resize(MAX_LIGHTS * LIGHT, 0);
    (bytes, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture whose left half is red and right half blue, the bottom half twice as dark.
    fn picture(size: [u32; 2]) -> Vec<u8> {
        let mut bgra = Vec::new();
        for y in 0..size[1] {
            for x in 0..size[0] {
                let level = if y < size[1] / 2 { 200 } else { 100 };
                bgra.extend(if x < size[0] / 2 { [0, 0, level, 255] } else { [level, 0, 0, 255] });
            }
        }
        bgra
    }

    #[test]
    fn each_cell_is_the_colour_of_its_part_of_the_picture() {
        for size in [[640, 360], [1920, 1080], [6, 4]] {
            let colors = cell_colors(&picture(size), size);
            let near = |a: f32, b: f32| (a - b).abs() < 0.01;
            assert!(near(colors[0][0], 200.0 / 255.0) && near(colors[0][2], 0.0), "top-left is red: {:?} at {size:?}", colors[0]);
            assert!(near(colors[1][2], 200.0 / 255.0) && near(colors[1][0], 0.0), "top-right is blue");
            assert!(near(colors[2][0], 100.0 / 255.0), "bottom-left is darker red");
            assert!(near(colors[3][2], 100.0 / 255.0), "bottom-right is darker blue");
        }
    }

    #[test]
    fn a_picture_that_is_not_all_there_gives_no_light() {
        assert_eq!(cell_colors(&[1, 2, 3], [640, 360]), [[0.0; 3]; CELLS]);
        assert_eq!(cell_colors(&[], [0, 0]), [[0.0; 3]; CELLS]);
    }

    #[test]
    fn the_cells_are_in_reading_order() {
        assert_eq!(cell_centre(0), [0.25, 0.25]);
        assert_eq!(cell_centre(1), [0.75, 0.25]);
        assert_eq!(cell_centre(2), [0.25, 0.75]);
        assert_eq!(cell_centre(CELLS - 1), [0.75, 0.75]);
    }

    #[test]
    fn the_lights_follow_the_picture_without_jumping() {
        let mut shown = [[0.0; 3]; CELLS];
        let white = [[1.0; 3]; CELLS];
        follow(&mut shown, &white);
        assert_eq!(shown[0], [FOLLOW; 3]);
        for _ in 0..40 {
            follow(&mut shown, &white);
        }
        assert!(shown[0][0] > 0.999, "and arrive");
    }

    fn glow(page: u32, x: f64) -> Glow {
        Glow { page, cell: 0, position: [x, 64.0, 0.0], normal: [0.0, 0.0, 1.0], reach: 8.0 }
    }

    fn floats(bytes: &[u8], light: usize) -> Vec<f32> {
        bytes[light * LIGHT..(light + 1) * LIGHT].chunks(4).map(|four| f32::from_le_bytes(four.try_into().unwrap())).collect()
    }

    #[test]
    fn lights_are_packed_nearest_first_relative_to_the_camera() {
        let glows = [glow(1, 30.0), glow(2, 12.0), glow(3, 20.0)];
        let (bytes, count) = pack(&glows, |glow| Some([glow.page as f32 / 10.0, 0.5, 0.0]), [10.0, 64.0, 0.0]);
        assert_eq!((count, bytes.len()), (3, MAX_LIGHTS * LIGHT));
        assert_eq!(floats(&bytes, 0), [2.0, 0.0, 0.0, 8.0, 0.0, 0.0, 1.0, 0.0, 0.2, 0.5, 0.0, 0.0], "page 2 is nearest");
        assert_eq!(floats(&bytes, 1)[0], 10.0);
        assert_eq!(floats(&bytes, 2)[0], 20.0);
        assert!(bytes[3 * LIGHT..].iter().all(|byte| *byte == 0), "the rest of the list is empty");
    }

    #[test]
    fn dark_cells_and_pages_without_a_picture_take_no_place_and_the_list_has_an_end() {
        let glows: Vec<Glow> = (0..MAX_LIGHTS as u32 + 20).map(|page| glow(page, f64::from(page))).collect();
        let (_, count) = pack(&glows, |glow| (glow.page % 2 == 0).then_some([1.0; 3]), [0.0, 64.0, 0.0]);
        assert_eq!(count as usize, (MAX_LIGHTS + 20) / 2, "odd pages have no picture");
        let (bytes, count) = pack(&glows, |_| Some([1.0; 3]), [0.0, 64.0, 0.0]);
        assert_eq!(count as usize, MAX_LIGHTS);
        assert_eq!(floats(&bytes, MAX_LIGHTS - 1)[0], (MAX_LIGHTS - 1) as f32, "the furthest are the ones left out");
        let (_, count) = pack(&glows, |_| Some([0.0; 3]), [0.0, 64.0, 0.0]);
        assert_eq!(count, 0, "black throws no light");
    }

    #[test]
    fn a_painted_colour_becomes_linear_light() {
        let linear = to_linear([0.0, 0.5, 1.0]);
        assert!(linear[0] == 0.0 && (linear[1] - 0.214).abs() < 0.001 && (linear[2] - 1.0).abs() < 1e-6, "{linear:?}");
    }
}
