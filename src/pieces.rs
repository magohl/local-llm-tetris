//! Tetromino definitions, rotation states (SRS) and wall-kick tables.

use crate::constants::COLS;

/// The seven tetrominoes. Order also indexes the color/palette tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    I = 0,
    O = 1,
    T = 2,
    S = 3,
    Z = 4,
    J = 5,
    L = 6,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::I,
        Kind::O,
        Kind::T,
        Kind::S,
        Kind::Z,
        Kind::J,
        Kind::L,
    ];

    pub fn from_u8(v: u8) -> Kind {
        Kind::ALL[(v as usize).min(6)]
    }

    /// Side length of the rotation box this piece lives in (4 for I, 2 for O, else 3).
    pub fn box_size(self) -> i32 {
        match self {
            Kind::I => 4,
            Kind::O => 2,
            _ => 3,
        }
    }

    /// Column at which this piece spawns (horizontally centred).
    pub fn spawn_x(self) -> i32 {
        ((COLS as i32) - self.box_size()) / 2
    }
}

/// Spawn-state (rotation 0) cells of each piece, `x` right / `y` down.
const BASE: [[(i32, i32); 4]; 7] = [
    [(0, 1), (1, 1), (2, 1), (3, 1)], // I
    [(0, 0), (1, 0), (0, 1), (1, 1)], // O
    [(1, 0), (0, 1), (1, 1), (2, 1)], // T
    [(1, 0), (2, 0), (0, 1), (1, 1)], // S
    [(0, 0), (1, 0), (1, 1), (2, 1)], // Z
    [(0, 0), (0, 1), (1, 1), (2, 1)], // J
    [(2, 0), (0, 1), (1, 1), (2, 1)], // L
];

/// The four cells of `kind` in rotation state `rot`, relative to the box origin.
///
/// Rotations are derived from the spawn state by rotating inside the box, which keeps
/// every piece self-consistent with the Super Rotation System.
pub fn cells(kind: Kind, rot: u32) -> [(i32, i32); 4] {
    let base = BASE[kind as usize];
    let s = kind.box_size();
    let r = rot & 3;
    let mut out = [(0i32, 0i32); 4];
    for i in 0..4 {
        let (x, y) = base[i];
        out[i] = match r {
            0 => (x, y),
            1 => (s - 1 - y, x),
            2 => (s - 1 - x, s - 1 - y),
            _ => (y, s - 1 - x),
        };
    }
    out
}

/// A tetromino in play: kind, rotation state and box origin on the grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece {
    pub kind: Kind,
    pub rot: u32,
    pub x: i32,
    pub y: i32,
}

impl Piece {
    pub fn new(kind: Kind) -> Piece {
        Piece {
            kind,
            rot: 0,
            x: kind.spawn_x(),
            y: crate::constants::HIDDEN as i32 - 1,
        }
    }

    /// Absolute grid cells of this piece.
    pub fn occupied(&self) -> [(i32, i32); 4] {
        let c = cells(self.kind, self.rot);
        let mut out = [(0i32, 0i32); 4];
        for i in 0..4 {
            out[i] = (c[i].0 + self.x, c[i].1 + self.y);
        }
        out
    }

    pub fn translated(&self, dx: i32, dy: i32) -> Piece {
        Piece {
            kind: self.kind,
            rot: self.rot,
            x: self.x + dx,
            y: self.y + dy,
        }
    }

    pub fn rotated(&self, dr: u32) -> Piece {
        Piece {
            kind: self.kind,
            rot: (self.rot + dr) & 3,
            x: self.x,
            y: self.y,
        }
    }

    /// Centre of the rotation box, used for T-spin corner checks.
    pub fn centre(&self) -> (i32, i32) {
        (self.x + 1, self.y + 1)
    }
}


/// Wall-kick offsets in (x, y-down) order for JLSTZ pieces.
const KICKS_JLSTZ: [[(i32, i32); 5]; 8] = [
    [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],  // 0 -> 1
    [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],    // 1 -> 0
    [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],    // 1 -> 2
    [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],  // 2 -> 1
    [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],     // 2 -> 3
    [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)], // 3 -> 2
    [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)], // 3 -> 0
    [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],     // 0 -> 3
];

/// Wall-kick offsets for the I piece.
const KICKS_I: [[(i32, i32); 5]; 8] = [
    [(0, 0), (-2, 0), (1, 0), (-2, 1), (1, -2)], // 0 -> 1
    [(0, 0), (2, 0), (-1, 0), (2, -1), (-1, 2)], // 1 -> 0
    [(0, 0), (-1, 0), (2, 0), (-1, -2), (2, 1)], // 1 -> 2
    [(0, 0), (1, 0), (-2, 0), (1, 2), (-2, -1)], // 2 -> 1
    [(0, 0), (2, 0), (-1, 0), (2, -1), (-1, 2)], // 2 -> 3
    [(0, 0), (-2, 0), (1, 0), (-2, 1), (1, -2)], // 3 -> 2
    [(0, 0), (1, 0), (-2, 0), (1, 2), (-2, -1)], // 3 -> 0
    [(0, 0), (-1, 0), (2, 0), (-1, -2), (2, 1)], // 0 -> 3
];

/// Offsets tried for a 180 degree rotation.
const KICKS_180: [(i32, i32); 5] = [(0, 0), (0, -1), (0, 1), (-1, 0), (1, 0)];

const NO_KICK: [(i32, i32); 5] = [(0, 0), (0, 0), (0, 0), (0, 0), (0, 0)];

fn transition_index(from: u32, to: u32) -> usize {
    match (from & 3, to & 3) {
        (0, 1) => 0,
        (1, 0) => 1,
        (1, 2) => 2,
        (2, 1) => 3,
        (2, 3) => 4,
        (3, 2) => 5,
        (3, 0) => 6,
        (0, 3) => 7,
        _ => 0,
    }
}

/// The five kick offsets to try for a rotation of `kind` from state `from` to `to`.
pub fn kicks(kind: Kind, from: u32, to: u32) -> &'static [(i32, i32); 5] {
    if kind == Kind::O {
        return &NO_KICK;
    }
    if (to.wrapping_sub(from)) & 3 == 2 {
        return &KICKS_180;
    }
    let idx = transition_index(from, to);
    if kind == Kind::I {
        &KICKS_I[idx]
    } else {
        &KICKS_JLSTZ[idx]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(kind: Kind, rot: u32) -> [(i32, i32); 4] {
        let mut c = cells(kind, rot);
        c.sort_unstable();
        c
    }

    #[test]
    fn rotations_match_the_classic_srs_tables() {
        assert_eq!(set(Kind::T, 0), [(0, 1), (1, 0), (1, 1), (2, 1)]);
        assert_eq!(set(Kind::T, 1), [(1, 0), (1, 1), (1, 2), (2, 1)]);
        assert_eq!(set(Kind::T, 2), [(0, 1), (1, 1), (1, 2), (2, 1)]);
        assert_eq!(set(Kind::T, 3), [(0, 1), (1, 0), (1, 1), (1, 2)]);
        assert_eq!(set(Kind::I, 0), [(0, 1), (1, 1), (2, 1), (3, 1)]);
        assert_eq!(set(Kind::I, 1), [(2, 0), (2, 1), (2, 2), (2, 3)]);
        assert_eq!(set(Kind::O, 0), set(Kind::O, 3));
    }

    #[test]
    fn four_rotations_return_to_the_start() {
        for k in Kind::ALL {
            for start in 0..4 {
                assert_eq!(set(k, start), set(k, start + 4), "piece {:?}", k);
            }
        }
    }

    #[test]
    fn every_rotation_stays_inside_its_box() {
        for k in Kind::ALL {
            for rot in 0..4 {
                let s = k.box_size();
                for (x, y) in cells(k, rot) {
                    assert!(x >= 0 && x < s && y >= 0 && y < s, "{:?} rot {}", k, rot);
                }
            }
        }
    }

    #[test]
    fn spawn_pieces_are_inside_the_walls() {
        for k in Kind::ALL {
            let p = Piece::new(k);
            for (x, _) in p.occupied() {
                assert!(x >= 0 && x < COLS as i32, "piece {:?}", k);
            }
        }
    }

    #[test]
    fn kicks_always_carry_a_noop_first_entry() {
        for k in Kind::ALL {
            for from in 0..4 {
                for to in 0..4 {
                    assert_eq!(kicks(k, from, to)[0], (0, 0));
                }
            }
        }
    }
}
