//! The faces of a block a screen can be on, and how a screen is turned on its face.
//!
//! The numbers are the Java mod's (`BlockSide`, `Rotation`, `ScreenShapeMode`,
//! `ScreenLinkMode`): they are what a screen saves, so they must not move.

/// A block position, or a whole-block direction.
pub type Pos = [i32; 3];

/// A face of a block, in the game's own order (down, up, north, south, west, east).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Side {
    Bottom = 0,
    Top = 1,
    North = 2,
    South = 3,
    West = 4,
    East = 5,
}

impl Side {
    pub const ALL: [Side; 6] = [Side::Bottom, Side::Top, Side::North, Side::South, Side::West, Side::East];

    /// The side a saved number names; none for a number that names no side.
    pub fn from_index(index: u8) -> Option<Side> {
        Side::ALL.get(index as usize).copied()
    }

    pub fn index(self) -> u8 {
        self as u8
    }

    /// The face across the block.
    pub fn opposite(self) -> Side {
        Side::ALL[(self as usize) ^ 1]
    }

    /// Which way a screen on this face looks: out of the block.
    pub fn forward(self) -> Pos {
        match self {
            Side::Bottom => [0, -1, 0],
            Side::Top => [0, 1, 0],
            Side::North => [0, 0, -1],
            Side::South => [0, 0, 1],
            Side::West => [-1, 0, 0],
            Side::East => [1, 0, 0],
        }
    }

    /// The picture's upward direction on this face, before the screen's own rotation.
    /// On the floor and the ceiling it is north.
    pub fn up(self) -> Pos {
        match self {
            Side::Bottom | Side::Top => [0, 0, -1],
            _ => [0, 1, 0],
        }
    }

    /// The picture's rightward direction on this face, before the screen's own rotation: as
    /// someone facing the screen sees it on the walls and from above. Underneath (`Bottom`)
    /// it is the same as on top, so seen from below the plane is mirrored; the Java mod has
    /// it so, and whoever turns a point of the screen into a picture's pixel allows for it.
    pub fn right(self) -> Pos {
        match self {
            Side::Bottom | Side::Top | Side::South => [1, 0, 0],
            Side::North => [-1, 0, 0],
            Side::West => [0, 0, 1],
            Side::East => [0, 0, -1],
        }
    }
}

/// `pos` moved `steps` blocks along `way`.
pub fn offset(pos: Pos, way: Pos, steps: i32) -> Pos {
    [pos[0] + way[0] * steps, pos[1] + way[1] * steps, pos[2] + way[2] * steps]
}

/// How a screen's picture is turned on its face, counter-clockwise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Rotation {
    #[default]
    R0 = 0,
    R90 = 1,
    R180 = 2,
    R270 = 3,
}

impl Rotation {
    pub const ALL: [Rotation; 4] = [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270];

    pub fn from_index(index: u8) -> Option<Rotation> {
        Rotation::ALL.get(index as usize).copied()
    }

    pub fn index(self) -> u8 {
        self as u8
    }

    pub fn degrees(self) -> f32 {
        self.index() as f32 * 90.0
    }

    /// A quarter turn either way: the picture's width runs along the screen's height.
    pub fn is_vertical(self) -> bool {
        matches!(self, Rotation::R90 | Rotation::R270)
    }

    /// The next rotation, as the configuration screen's button steps through them.
    pub fn next(self) -> Rotation {
        Rotation::ALL[(self.index() as usize + 1) % 4]
    }
}

/// How the edge of a screen that is not a rectangle is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ShapeMode {
    /// Whole blocks: a staircase.
    #[default]
    None = 0,
    /// Each step cut by a slope one block long.
    SmoothOne = 1,
    /// Each run of steps joined by one long slope.
    SmoothLong = 2,
}

impl ShapeMode {
    pub const ALL: [ShapeMode; 3] = [ShapeMode::None, ShapeMode::SmoothOne, ShapeMode::SmoothLong];

    pub fn from_index(index: u8) -> Option<ShapeMode> {
        ShapeMode::ALL.get(index as usize).copied()
    }

    pub fn index(self) -> u8 {
        self as u8
    }

    pub fn next(self) -> ShapeMode {
        ShapeMode::ALL[(self.index() as usize + 1) % 3]
    }
}

/// How the screens of a linked group share one picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum LinkMode {
    /// The screens are treated as touching: the gaps between them take none of the picture.
    #[default]
    Joined = 0,
    /// The picture covers the group's whole extent: what falls in a gap is not shown.
    Spaced = 1,
}

impl LinkMode {
    pub fn from_index(index: u8) -> Option<LinkMode> {
        [LinkMode::Joined, LinkMode::Spaced].get(index as usize).copied()
    }

    pub fn index(self) -> u8 {
        self as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cross(a: Pos, b: Pos) -> Pos {
        [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
    }

    #[test]
    fn the_numbers_are_the_ones_a_java_screen_saved() {
        assert_eq!(Side::ALL.map(Side::index), [0, 1, 2, 3, 4, 5]);
        assert_eq!(Side::from_index(3), Some(Side::South));
        assert_eq!(Side::from_index(6), None);
        assert_eq!(Rotation::from_index(2), Some(Rotation::R180));
        assert_eq!(Rotation::from_index(4), None);
        assert_eq!(ShapeMode::from_index(2), Some(ShapeMode::SmoothLong));
        assert_eq!(LinkMode::from_index(1), Some(LinkMode::Spaced));
        assert_eq!(LinkMode::from_index(2), None);
    }

    #[test]
    fn each_side_has_its_opposite_and_looks_out_of_the_block() {
        for side in Side::ALL {
            assert_eq!(side.opposite().opposite(), side);
            assert_eq!(side.opposite().forward(), side.forward().map(|v| -v));
        }
        assert_eq!(Side::North.opposite(), Side::South);
        assert_eq!(Side::Top.forward(), [0, 1, 0]);
    }

    #[test]
    fn up_and_right_lie_in_the_face() {
        for side in Side::ALL {
            let dot = |a: Pos, b: Pos| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            assert_eq!(dot(side.up(), side.forward()), 0, "{side:?}");
            assert_eq!(dot(side.right(), side.forward()), 0, "{side:?}");
            assert_eq!(dot(side.up(), side.right()), 0, "{side:?}");
        }
        // On a wall, someone facing the screen has `right` at their right: up x right points
        // at them, out of the block.
        for side in [Side::North, Side::South, Side::West, Side::East] {
            assert_eq!(cross(side.up(), side.right()), side.forward().map(|v| -v), "{side:?}");
        }
    }

    #[test]
    fn rotations_step_round_and_know_when_they_swap_the_sides() {
        assert_eq!(Rotation::R270.next(), Rotation::R0);
        assert_eq!(Rotation::R90.degrees(), 90.0);
        assert!(Rotation::R90.is_vertical() && Rotation::R270.is_vertical());
        assert!(!Rotation::R0.is_vertical() && !Rotation::R180.is_vertical());
        assert_eq!(ShapeMode::SmoothLong.next(), ShapeMode::None);
        assert_eq!(offset([1, 2, 3], Side::East.forward(), 4), [5, 2, 3]);
    }
}
