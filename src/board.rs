//! The playfield grid: collision, locking and line clearing.

use crate::constants::{COLS, HIDDEN, ROWS, TOTAL_ROWS};
use crate::pieces::Piece;

/// `grid[y][x]` is 0 when empty, otherwise `kind as u8 + 1`.
///
/// Rows `0..HIDDEN` are the hidden buffer above the visible field.
#[derive(Clone)]
pub struct Board {
    pub grid: [[u8; COLS]; TOTAL_ROWS],
}

impl Board {
    pub fn new() -> Board {
        Board {
            grid: [[0; COLS]; TOTAL_ROWS],
        }
    }

    /// Cell value, or -1 when the coordinate is outside the playfield.
    #[inline]
    pub fn get(&self, x: i32, y: i32) -> i32 {
        if x < 0 || x as usize >= COLS || y < 0 || y as usize >= TOTAL_ROWS {
            return -1;
        }
        self.grid[y as usize][x as usize] as i32
    }

    pub fn set(&mut self, x: i32, y: i32, v: u8) {
        if x >= 0 && (x as usize) < COLS && y >= 0 && (y as usize) < TOTAL_ROWS {
            self.grid[y as usize][x as usize] = v;
        }
    }

    /// True when the piece overlaps a wall, the floor or another block.
    pub fn collides(&self, p: &Piece) -> bool {
        for (x, y) in p.occupied() {
            if x < 0 || x >= COLS as i32 || y >= TOTAL_ROWS as i32 {
                return true;
            }
            if y >= 0 && self.grid[y as usize][x as usize] != 0 {
                return true;
            }
        }
        false
    }

    /// True when the piece overlaps another block but not the walls/floor.
    pub fn collides_blocks_only(&self, p: &Piece) -> bool {
        for (x, y) in p.occupied() {
            if x < 0 || x >= COLS as i32 || y < 0 || y >= TOTAL_ROWS as i32 {
                continue;
            }
            if self.grid[y as usize][x as usize] != 0 {
                return true;
            }
        }
        false
    }

    /// Write the piece into the grid. Returns false if it overlapped something.
    pub fn lock(&mut self, p: &Piece) -> bool {
        if self.collides(p) {
            return false;
        }
        let v = p.kind as u8 + 1;
        for (x, y) in p.occupied() {
            self.set(x, y, v);
        }
        true
    }

    /// Rows (absolute grid indices) that are completely filled.
    pub fn full_rows(&self) -> Vec<usize> {
        let mut out = Vec::new();
        for y in 0..TOTAL_ROWS {
            if self.grid[y].iter().all(|&c| c != 0) {
                out.push(y);
            }
        }
        out
    }

    /// Remove the given rows, dropping everything above them down by `rows.len()`.
    pub fn remove_rows(&mut self, rows: &[usize]) {
        if rows.is_empty() {
            return;
        }
        let mut kept = [[0u8; COLS]; TOTAL_ROWS];
        let mut n = 0;
        for y in 0..TOTAL_ROWS {
            if !rows.contains(&y) {
                kept[n] = self.grid[y];
                n += 1;
            }
        }
        for row in self.grid.iter_mut() {
            *row = [0; COLS];
        }
        for i in 0..n {
            self.grid[TOTAL_ROWS - n + i] = kept[i];
        }
    }

    /// True when the visible field holds no blocks at all.
    pub fn is_empty(&self) -> bool {
        self.grid[HIDDEN..].iter().all(|r| r.iter().all(|&c| c == 0))
    }

    /// Number of blocks on the visible field.
    pub fn visible_blocks(&self) -> usize {
        self.grid[HIDDEN..]
            .iter()
            .flat_map(|r| r.iter())
            .filter(|&&c| c != 0)
            .count()
    }

    /// Index of the top-most occupied row (TOTAL_ROWS when the board is empty).
    pub fn top_row(&self) -> usize {
        for y in 0..TOTAL_ROWS {
            if self.grid[y].iter().any(|&c| c != 0) {
                return y;
            }
        }
        TOTAL_ROWS
    }

    /// The visible slice of the grid, top row first.
    pub fn visible(&self) -> &[[u8; COLS]] {
        &self.grid[HIDDEN..]
    }

    /// Row contents for a plain 20-row view (used by tests).
    pub fn visible_rows(&self) -> [[u8; COLS]; ROWS] {
        let mut out = [[0u8; COLS]; ROWS];
        out.copy_from_slice(&self.grid[HIDDEN..]);
        out
    }
}

impl Default for Board {
    fn default() -> Self {
        Board::new()
    }
}
