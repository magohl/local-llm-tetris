//! Software renderer: everything you see is rasterised by Rust into an RGBA buffer.
//!
//! There is no DOM/canvas drawing API involved at all - the browser only ever uploads
//! the finished pixel buffer, which keeps the JS glue down to a few dozen lines.

use crate::constants::*;
use crate::font::{glyph, GH, GW};
use crate::game::{msg_text, Game};
use crate::pieces::{cells, Kind};

pub type Rgb = [u8; 3];

/// Block colours, indexed by `grid value - 1` (index 0 is a placeholder for empty).
pub const PALETTE: [Rgb; 8] = [
    [0, 0, 0],        // empty
    [86, 222, 232],   // I
    [249, 214, 84],   // O
    [178, 111, 238],  // T
    [104, 216, 116],  // S
    [243, 106, 112],  // Z
    [102, 145, 255],  // J
    [252, 158, 74],   // L
];

const BG_TOP: Rgb = [13, 15, 28];
const BG_BOT: Rgb = [26, 22, 46];
const PANEL: Rgb = [25, 29, 51];
const PANEL_EDGE: Rgb = [58, 68, 108];
const WELL: Rgb = [9, 10, 20];
const GRID: Rgb = [32, 38, 62];
const TEXT: Rgb = [214, 224, 250];
const DIM: Rgb = [124, 136, 172];
const ACCENT: Rgb = [126, 214, 255];

/// Multiply a colour by a brightness factor.
#[inline]
pub fn shade(c: Rgb, f: f32) -> Rgb {
    [
        clamp8((c[0] as f32 * f) as i32),
        clamp8((c[1] as f32 * f) as i32),
        clamp8((c[2] as f32 * f) as i32),
    ]
}

/// Linear blend, `t` in 0..=255.
#[inline]
pub fn mix(a: Rgb, b: Rgb, t: u32) -> Rgb {
    let t = t.min(255);
    [
        ((a[0] as u32 * (255 - t) + b[0] as u32 * t) / 255) as u8,
        ((a[1] as u32 * (255 - t) + b[1] as u32 * t) / 255) as u8,
        ((a[2] as u32 * (255 - t) + b[2] as u32 * t) / 255) as u8,
    ]
}

/// Brighten/darken uniformly.
#[inline]
pub fn add(c: Rgb, v: i32) -> Rgb {
    [
        clamp8(c[0] as i32 + v),
        clamp8(c[1] as i32 + v),
        clamp8(c[2] as i32 + v),
    ]
}

#[inline]
fn clamp8(v: i32) -> u8 {
    if v < 0 {
        0
    } else if v > 255 {
        255
    } else {
        v as u8
    }
}

/// A raw RGBA8 framebuffer.
pub struct Image {
    pub buf: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

impl Image {
    pub fn new(w: u32, h: u32) -> Image {
        let mut buf = vec![0u8; (w * h * 4) as usize];
        for i in (3..buf.len()).step_by(4) {
            buf[i] = 255;
        }
        Image { buf, w, h }
    }

    #[inline]
    fn inside(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as u32) < self.w && (y as u32) < self.h
    }

    #[inline]
    fn off(&self, x: i32, y: i32) -> usize {
        ((y as u32 * self.w + x as u32) as usize) * 4
    }

    /// Opaque pixel write (clipped to the surface).
    #[inline]
    pub fn put(&mut self, x: i32, y: i32, c: Rgb) {
        if !self.inside(x, y) {
            return;
        }
        let o = self.off(x, y);
        self.buf[o] = c[0];
        self.buf[o + 1] = c[1];
        self.buf[o + 2] = c[2];
        self.buf[o + 3] = 255;
    }

    /// Alpha-blended pixel write, `a` in 0..=255.
    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, c: Rgb, a: u32) {
        if a == 0 || !self.inside(x, y) {
            return;
        }
        let o = self.off(x, y);
        if a >= 255 {
            self.buf[o] = c[0];
            self.buf[o + 1] = c[1];
            self.buf[o + 2] = c[2];
            return;
        }
        for k in 0..3 {
            let d = self.buf[o + k] as u32;
            self.buf[o + k] = ((d * (255 - a) + c[k] as u32 * a) / 255) as u8;
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgb) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.put(xx, yy, c);
            }
        }
    }

    pub fn blend_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgb, a: u32) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.blend(xx, yy, c, a);
            }
        }
    }

    /// Rectangle filled with a top-to-bottom colour ramp.
    pub fn vgrad(&mut self, x: i32, y: i32, w: i32, h: i32, top: Rgb, bot: Rgb) {
        let span = (h - 1).max(1) as u32;
        for row in 0..h {
            let t = row.max(0) as u32 * 255 / span;
            let c = mix(top, bot, t);
            for xx in x..x + w {
                self.put(xx, y + row, c);
            }
        }
    }

    pub fn stroke_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgb, t: i32) {
        for i in 0..t {
            for xx in x..x + w {
                self.put(xx, y + i, c);
                self.put(xx, y + h - 1 - i, c);
            }
            for yy in y..y + h {
                self.put(x + i, yy, c);
                self.put(x + w - 1 - i, yy, c);
            }
        }
    }

    /// Panel with chamfered corners: filled body plus a lit top edge.
    pub fn panel(&mut self, x: i32, y: i32, w: i32, h: i32, fill: Rgb, edge: Rgb) {
        self.fill_rect(x, y, w, h, fill);
        self.stroke_rect(x, y, w, h, edge, 1);
        let c = 5;
        for i in 0..c {
            self.put(x + i, y, fill);
            self.put(x + i, y + 1, fill);
            self.put(x + w - 1 - i, y + h - 1, fill);
            self.put(x + w - 1 - i, y + h - 2, fill);
        }
        for i in 0..c {
            self.put(x, y + i, fill);
            self.put(x + 1, y + i, fill);
            self.put(x + w - 1, y + h - 1 - i, fill);
            self.put(x + w - 2, y + h - 1 - i, fill);
        }
        self.hline(x + c, y, w - 2 * c, edge);
    }

    pub fn hline(&mut self, x: i32, y: i32, w: i32, c: Rgb) {
        for xx in x..x + w {
            self.put(xx, y, c);
        }
    }
}



// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

/// Draw text with the built-in 5x7 bitmap font; returns the pixel width.
pub fn text(img: &mut Image, x: i32, y: i32, s: &str, c: Rgb, scale: u32) -> i32 {
    let mut cx = x;
    for ch in s.bytes() {
        let g = glyph(ch);
        for (row, bits) in g.iter().enumerate() {
            for col in 0..GW {
                if bits & (1 << (4 - col)) != 0 {
                    img.fill_rect(
                        cx + (col * scale) as i32,
                        y + (row as u32 * scale) as i32,
                        scale as i32,
                        scale as i32,
                        c,
                    );
                }
            }
        }
        cx += (GW * scale + scale) as i32;
    }
    if !s.is_empty() {
        cx -= scale as i32;
    }
    cx - x
}

/// Draw text with a hard drop shadow so it stays legible over the stack.
pub fn text_shadow(img: &mut Image, x: i32, y: i32, s: &str, c: Rgb, scale: u32) {
    text(img, x + scale as i32, y + scale as i32, s, [6, 6, 14], scale);
    text(img, x, y, s, c, scale);
}

/// Horizontally centred on `cx`.
pub fn text_center(img: &mut Image, cx: i32, y: i32, s: &str, c: Rgb, scale: u32) {
    let w = crate::font::measure(s, scale) as i32;
    text_shadow(img, cx - w / 2, y, s, c, scale);
}

/// Integer drawn right-aligned against pixel column `right`.
pub fn text_num(img: &mut Image, right: i32, y: i32, n: u64, c: Rgb, scale: u32) {
    let (buf, i) = digits(n);
    let s = std::str::from_utf8(&buf[i..]).unwrap_or("0");
    let w = crate::font::measure(s, scale) as i32;
    text(img, right - w, y, s, c, scale);
}

/// Milliseconds as "M:SS" in a fixed-width buffer.
fn clock(ms: f32) -> ([u8; 8], usize) {
    let total = if ms.is_finite() { ms.max(0.0) / 1000.0 } else { 0.0 } as u32;
    let (m, s) = (total / 60, total % 60);
    let mut b = [b' '; 8];
    b[0] = b'0' + (m / 10).min(9) as u8;
    b[1] = b'0' + (m % 10) as u8;
    b[2] = b':';
    b[3] = b'0' + (s / 10) as u8;
    b[4] = b'0' + (s % 10) as u8;
    (b, 5)
}

// ---------------------------------------------------------------------------
// Blocks
// ---------------------------------------------------------------------------

/// One bevelled tetromino cell with its top-left pixel at `px, py`.
pub fn block(img: &mut Image, px: i32, py: i32, size: i32, col: Rgb) {
    let b = (size / 8).max(2);
    img.vgrad(px, py, size, size, shade(col, 1.10), shade(col, 0.80));
    img.fill_rect(px, py, size, b, shade(col, 1.38));
    img.fill_rect(px, py, b, size, shade(col, 1.20));
    img.fill_rect(px, py + size - b, size, b, shade(col, 0.56));
    img.fill_rect(px + size - b, py, b, size, shade(col, 0.70));
    img.fill_rect(px + b, py + b, b, b, add(col, 55));
    img.stroke_rect(px, py, size, size, [10, 12, 22], 1);
}

/// Translucent landing preview.
pub fn block_ghost(img: &mut Image, px: i32, py: i32, size: i32, col: Rgb) {
    let i = 4;
    img.blend_rect(px + i, py + i, size - 2 * i, size - 2 * i, col, 30);
    img.stroke_rect(px + i, py + i, size - 2 * i, size - 2 * i, add(col, 20), 2);
}

/// Flattened block for previews that are not currently selectable.
pub fn block_dim(img: &mut Image, px: i32, py: i32, size: i32, col: Rgb) {
    let c = mix(col, PANEL, 165);
    img.vgrad(px, py, size, size, shade(c, 1.0), shade(c, 0.82));
    img.stroke_rect(px, py, size, size, [12, 14, 24], 1);
}

/// Colour for a grid value (0 = empty).
pub fn color_of(v: u8) -> Rgb {
    PALETTE[(v as usize).min(7)]
}

/// Draw a piece centred inside a preview box.
pub fn preview_piece(
    img: &mut Image,
    kind: Kind,
    bx: i32,
    by: i32,
    bw: i32,
    bh: i32,
    size: i32,
    dim: bool,
) {
    let cs = cells(kind, 0);
    let min_x = cs.iter().map(|c| c.0).min().unwrap_or(0);
    let min_y = cs.iter().map(|c| c.1).min().unwrap_or(0);
    let max_x = cs.iter().map(|c| c.0).max().unwrap_or(0);
    let max_y = cs.iter().map(|c| c.1).max().unwrap_or(0);
    let w = (max_x - min_x + 1) * size;
    let h = (max_y - min_y + 1) * size;
    let ox = bx + (bw - w) / 2;
    let oy = by + (bh - h) / 2;
    let col = color_of(kind as u8 + 1);
    for (cx, cy) in cs.iter() {
        let px = ox + (cx - min_x) * size;
        let py = oy + (cy - min_y) * size;
        if dim {
            block_dim(img, px, py, size, col);
        } else {
            block(img, px, py, size, col);
        }
    }
}

// ---------------------------------------------------------------------------
// Scene
// ---------------------------------------------------------------------------

/// Render one complete frame of the game into the framebuffer.
pub fn draw(g: &Game, img: &mut Image) {
    img.vgrad(0, 0, FB_W as i32, FB_H as i32, BG_TOP, BG_BOT);

    // Integer-only camera wobble (no libm needed in the wasm build).
    let sh = g.shake.max(0.0) as i32;
    let (sx, sy) = if sh > 0 {
        let ph = (g.time_ms as i32) / 24;
        (((ph * 7) % 5) * sh / 10, ((ph * 3) % 5) * sh / 14)
    } else {
        (0, 0)
    };

    let bx = BOARD_X as i32 + sx;
    let by = BOARD_Y as i32 + sy;

    draw_well(g, img, bx, by);
    draw_stack(g, img, bx, by);
    if g.phase == Phase::Playing {
        draw_active(g, img, bx, by);
    }
    draw_clear_fx(g, img, bx, by);
    draw_frame(img, bx, by);
    draw_panel(g, img);
    draw_popups(g, img, bx, by);

    match g.phase {
        Phase::Ready => draw_title(g, img),
        Phase::Paused => draw_paused(g, img),
        Phase::GameOver => draw_over(g, img),
        _ => {}
    }
}

/// The dark playfield interior plus its grid.
fn draw_well(g: &Game, img: &mut Image, bx: i32, by: i32) {
    let bw = BOARD_W as i32;
    let bh = BOARD_H as i32;
    img.panel(bx - 6, by - 6, bw + 12, bh + 12, PANEL, PANEL_EDGE);
    img.vgrad(bx, by, bw, bh, WELL, mix(WELL, [26, 24, 48], 90));

    let top = g.board.top_row();
    if top < HIDDEN + 6 && g.phase != Phase::Ready {
        let a = ((HIDDEN + 6 - top) as u32 * 22).min(110);
        img.blend_rect(bx, by, bw, 6 * CELL as i32, [220, 60, 70], a);
    }

    for x in 1..COLS as i32 {
        for y in 0..bh {
            img.blend(bx + x * CELL as i32, by + y, GRID, 140);
        }
    }
    for y in 1..ROWS as i32 {
        for x in 0..bw {
            img.blend(bx + x, by + y * CELL as i32, GRID, 100);
        }
    }
}

/// All settled blocks.
fn draw_stack(g: &Game, img: &mut Image, bx: i32, by: i32) {
    for gy in HIDDEN..TOTAL_ROWS {
        let py = by + (gy - HIDDEN) as i32 * CELL as i32;
        for gx in 0..COLS {
            let v = g.board.grid[gy][gx];
            if v == 0 {
                continue;
            }
            if g.phase == Phase::Clearing && g.clear_rows[..g.n_clear].contains(&gy) {
                continue; // redrawn by draw_clear_fx
            }
            block(img, bx + gx as i32 * CELL as i32, py, CELL as i32, color_of(v));
        }
    }
}

/// Ghost piece plus the falling piece.
fn draw_active(g: &Game, img: &mut Image, bx: i32, by: i32) {
    let size = CELL as i32;
    let col = color_of(g.piece.kind as u8 + 1);
    let drop = g.ghost_y() - g.piece.y;
    // A cell of the buffer rows above the field must not be painted over the
    // frame: it has not entered the playfield yet.
    let entered = |y: &i32| *y >= HIDDEN as i32;

    if drop > 0 {
        for (ax, ay) in g.piece.occupied().iter().filter(|(_, ay)| entered(ay)) {
            block_ghost(
                img,
                bx + ax * size,
                by + (ay + drop - HIDDEN as i32) * size,
                size,
                col,
            );
        }
    }

    // Lock-delay urgency: the outline brightens as the piece is about to stick.
    if g.grounded && g.lock_timer > 60.0 {
        let a = ((g.lock_timer / LOCK_DELAY).clamp(0.0, 1.0) * 150.0) as u32;
        for (ax, ay) in g.piece.occupied().iter().filter(|(_, ay)| entered(ay)) {
            img.stroke_rect(
                bx + ax * size,
                by + (ay - HIDDEN as i32) * size,
                size,
                size,
                add(col, 90),
                if a > 110 { 2 } else { 1 },
            );
        }
        if a > 40 {
            img.blend_rect(bx, by, BOARD_W as i32, BOARD_H as i32, [120, 40, 50], a / 8);
        }
    }

    for (ax, ay) in g.piece.occupied().iter().filter(|(_, ay)| entered(ay)) {
        block(img, bx + ax * size, by + (ay - HIDDEN as i32) * size, size, col);
    }
}

/// The white sweep that plays while rows are removed.
fn draw_clear_fx(g: &Game, img: &mut Image, bx: i32, by: i32) {
    if g.phase != Phase::Clearing || g.n_clear == 0 {
        return;
    }
    let t = (g.clear_timer / CLEAR_ANIM_MS).clamp(0.0, 1.0);
    let white = ((1.0 - t) * 200.0) as u32;
    for i in 0..g.n_clear {
        let gy = g.clear_rows[i];
        if gy < HIDDEN {
            continue;
        }
        let py = by + (gy - HIDDEN) as i32 * CELL as i32;
        for gx in 0..COLS {
            let col = color_of(g.board.grid[gy][gx]);
            block(img, bx + gx as i32 * CELL as i32, py, CELL as i32, col);
        }
        img.blend_rect(bx, py, BOARD_W as i32, CELL as i32, [255, 255, 255], white / 2);
        let band = ((1.0 - t) * (CELL as f32 * 0.8)) as i32;
        img.fill_rect(bx, py + CELL as i32 / 2 - band / 2, BOARD_W as i32, band.max(1), [255, 255, 255]);
    }
    if g.flash > 0.0 {
        let a = (g.flash * 0.35).min(70.0) as u32;
        img.blend_rect(bx, by, BOARD_W as i32, BOARD_H as i32, [255, 255, 255], a);
    }
}

/// Bright border drawn after the contents so the field reads as a well.
fn draw_frame(img: &mut Image, bx: i32, by: i32) {
    let bw = BOARD_W as i32;
    let bh = BOARD_H as i32;
    img.stroke_rect(bx - 2, by - 2, bw + 4, bh + 4, [10, 12, 24], 1);
    img.stroke_rect(bx - 7, by - 7, bw + 14, bh + 14, PANEL_EDGE, 1);
    let c = 14;
    for (cx, cy) in [
        (bx - 7, by - 7),
        (bx + bw + 6, by - 7),
        (bx - 7, by + bh + 6),
        (bx + bw + 6, by + bh + 6),
    ] {
        let dx = if cx < bx + bw / 2 { 1 } else { -1 };
        let dy = if cy < by + bh / 2 { 1 } else { -1 };
        for i in 0..c {
            img.put(cx + dx * i, cy, ACCENT);
            img.put(cx, cy + dy * i, ACCENT);
        }
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

fn label(img: &mut Image, x: i32, y: i32, s: &str) {
    text(img, x, y, s, DIM, 2);
}

fn stat_row(img: &mut Image, px: i32, py: i32, name: &str, value: u64, col: Rgb) {
    text(img, px + 8, py, name, DIM, 2);
    text_num(img, px + PANEL_W as i32 - 8, py - 1, value, col, 2);
}

fn draw_panel(g: &Game, img: &mut Image) {
    let px = PANEL_X as i32;
    let pw = PANEL_W as i32;

    // Hold
    label(img, px + 8, 24, "HOLD");
    img.panel(px, 44, pw, 76, PANEL, PANEL_EDGE);
    if g.has_hold {
        preview_piece(img, g.hold, px + 4, 44, pw - 8, 76, 16, !g.can_hold);
    }

    // Next
    label(img, px + 8, 134, "NEXT");
    let slot = 54;
    let nh = (NEXT_PREVIEW as i32) * slot + 8;
    img.panel(px, 154, pw, nh, PANEL, PANEL_EDGE);
    for (i, k) in g.preview(NEXT_PREVIEW).iter().enumerate() {
        let y = 158 + i as i32 * slot;
        let size = if i == 0 { 15 } else { 11 };
        preview_piece(img, *k, px + 4, y, pw - 8, slot - 4, size, false);
        if i == 0 {
            for x in px + 10..px + pw - 10 {
                img.blend(x, y + slot - 4, PANEL_EDGE, 200);
            }
        }
    }

    // Score block
    let sy = 154 + nh + 18;
    img.panel(px, sy - 8, pw, 52, mix(PANEL, [0, 0, 0], 45), PANEL_EDGE);
    label(img, px + 8, sy, "SCORE");
    text_num(img, px + pw - 8, sy + 17, g.score as u64, TEXT, 3);

    // Stats
    let mut y = sy + 62;
    stat_row(img, px, y, "BEST", g.best.max(g.score) as u64, ACCENT);
    y += 22;
    stat_row(img, px, y, "LINES", g.lines as u64, TEXT);
    y += 22;
    stat_row(img, px, y, "LEVEL", g.level as u64, TEXT);
    y += 22;
    let (cb, clen) = clock(g.time_ms);
    let ct = std::str::from_utf8(&cb[..clen]).unwrap_or("0:00");
    text(img, px + 8, y, "TIME", DIM, 2);
    text(img, px + pw - 8 - crate::font::measure(ct, 2) as i32, y - 1, ct, TEXT, 2);
    y += 22;
    stat_row(img, px, y, "PIECES", g.pieces_placed as u64, TEXT);
    y += 26;

    // Streak indicators
    for x in px + 4..px + pw - 4 {
        img.blend(x, y, PANEL_EDGE, 190);
    }
    y += 8;
    if g.b2b >= 1 {
        text_count(img, px + 8, y, "B2B X", g.b2b as u64, ACCENT, 2);
        y += 20;
    }
    if g.combo > 0 {
        text_count(img, px + 8, y, "COMBO X", g.combo as u64, [255, 196, 92], 2);
    }
}

// ---------------------------------------------------------------------------
// Popups
// ---------------------------------------------------------------------------

fn draw_popups(g: &Game, img: &mut Image, bx: i32, by: i32) {
    if g.phase == Phase::Ready || g.phase == Phase::GameOver {
        return;
    }
    let cx = bx + BOARD_W as i32 / 2;
    let mut y = by + BOARD_H as i32 / 2 - 60;

    if g.msg.timer > 0.0 && g.msg.timer < 1.0e6 {
        let t = (g.msg.timer / g.msg.total.max(1.0)).clamp(0.0, 1.0);
        let a = if t > 0.8 {
            ((1.0 - t) * 5.0 * 255.0) as u32
        } else {
            (t * 255.0).min(255.0) as u32
        };
        let rise = ((1.0 - t) * 14.0) as i32;
        let scale = if g.msg.big { 4 } else { 3 };
        let tint = if g.msg.big { [255, 228, 120] } else { TEXT };
        draw_text_faded_center(img, cx, y + rise, msg_text(&g.msg), tint, scale, a);
        y += scale as i32 * GH as i32 + 8;
    }
    if g.sub_msg.timer > 0.0 {
        let t = (g.sub_msg.timer / 1300.0).clamp(0.0, 1.0);
        let a = (t * 255.0) as u32;
        draw_text_faded_center(img, cx, y + 4, msg_text(&g.sub_msg), ACCENT, 2, a);
    }
}

fn draw_text_faded_center(img: &mut Image, cx: i32, y: i32, s: &str, c: Rgb, scale: u32, a: u32) {
    let w = crate::font::measure(s, scale) as i32;
    let mut xx = cx - w / 2;
    for ch in s.bytes() {
        let g = glyph(ch);
        for (row, bits) in g.iter().enumerate() {
            for col in 0..GW {
                if bits & (1 << (4 - col)) != 0 {
                    let px = xx + (col as u32 * scale) as i32;
                    let py = y + (row as u32 * scale) as i32;
                    img.fill_rect(px + 2, py + 2, scale as i32, scale as i32, [4, 4, 10]);
                    img.blend_rect(px, py, scale as i32, scale as i32, c, a);
                }
            }
        }
        xx += (GW * scale + scale) as i32;
    }
}

/// `KEY xN` with an integer counter, no formatting machinery.
fn text_count(img: &mut Image, x: i32, y: i32, key: &str, n: u64, c: Rgb, scale: u32) {
    text(img, x, y, key, c, scale);
    let (buf, i) = digits(n);
    let s = std::str::from_utf8(&buf[i..]).unwrap_or("0");
    text(img, x + crate::font::measure(key, scale) as i32, y, s, c, scale);
}

/// Render `n` into a fixed buffer, returning it and the first significant index.
fn digits(n: u64) -> ([u8; 20], usize) {
    let mut b = [b' '; 20];
    let mut v = n;
    let mut i = b.len();
    loop {
        i -= 1;
        b[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    (b, i)
}

// ---------------------------------------------------------------------------
// Overlays
// ---------------------------------------------------------------------------

fn blink(g: &Game, period: f32) -> bool {
    ((g.ui_time / period) as u32) % 2 == 0
}

fn draw_title(g: &Game, img: &mut Image) {
    img.blend_rect(0, 0, FB_W as i32, FB_H as i32, [5, 6, 14], 130);
    let cx = FB_W as i32 / 2;

    // Chromatic title
    let scale = 7u32;
    let word = "TETRIS";
    let w = crate::font::measure(word, scale) as i32;
    let mut x = cx - w / 2;
    for (i, ch) in word.char_indices() {
        let col = PALETTE[(i % 6) + 1];
        let mut tmp = [0u8; 4];
        let s: &str = ch.encode_utf8(&mut tmp);
        text(img, x + 3, 93, s, [4, 4, 10], scale);
        text(img, x, 90, s, col, scale);
        x += (GW * scale + scale) as i32;
    }

    text_center(img, cx, 160, "PURE RUST + WEBASSEMBLY", ACCENT, 2);
    text_center(img, cx, 184, "SOFTWARE RENDERED - ZERO JS GAME LOGIC", DIM, 2);

    let rows: [(&str, &str); 10] = [
        ("LEFT / RIGHT", "MOVE"),
        ("DOWN", "SOFT DROP"),
        ("SPACE", "HARD DROP"),
        ("UP / X", "ROTATE CW"),
        ("Z", "ROTATE CCW"),
        ("A", "ROTATE 180"),
        ("C / CTRL", "HOLD PIECE"),
        ("P", "PAUSE"),
        ("R", "RESTART"),
        ("ENTER", "START"),
    ];
    let left = cx - 150;
    let mut y = 236;
    for (k, v) in rows.iter() {
        text(img, left, y, k, ACCENT, 2);
        text(img, left + 156, y, v, TEXT, 2);
        y += 22;
    }

    if blink(g, 600.0) {
        text_center(img, cx, 480, "PRESS ENTER TO PLAY", [255, 240, 200], 3);
    }
    text_center(img, cx, 540, "SRS KICKS  7-BAG  HOLD  T-SPINS  LOCK DELAY", DIM, 2);
    text_center(img, cx, 566, "CLEAR 10 LINES TO LEVEL UP", DIM, 2);
}

fn draw_paused(g: &Game, img: &mut Image) {
    let bx = BOARD_X as i32;
    let by = BOARD_Y as i32;
    img.blend_rect(bx, by, BOARD_W as i32, BOARD_H as i32, [5, 6, 16], 170);
    let cx = bx + BOARD_W as i32 / 2;
    text_center(img, cx, by + BOARD_H as i32 / 2 - 60, "PAUSED", TEXT, 5);
    text_center(img, cx, by + BOARD_H as i32 / 2 - 10, "PRESS P TO RESUME", ACCENT, 2);
    if blink(g, 700.0) {
        text_center(img, cx, by + BOARD_H as i32 / 2 + 24, "R TO RESTART", DIM, 2);
    }
}

fn draw_over(g: &Game, img: &mut Image) {
    let bx = BOARD_X as i32;
    let by = BOARD_Y as i32;
    img.blend_rect(0, 0, FB_W as i32, FB_H as i32, [8, 4, 10], 150);
    let cx = bx + BOARD_W as i32 / 2;
    text_center(img, cx, by + 120, "GAME OVER", [255, 120, 110], 4);
    text_center(img, cx, by + 160, msg_text(&g.msg), DIM, 2);

    let mut y = by + 210;
    let rows: [(&str, u64); 6] = [
        ("SCORE", g.score as u64),
        ("BEST", g.best.max(g.score) as u64),
        ("LINES", g.lines as u64),
        ("LEVEL", g.level as u64),
        ("PIECES", g.pieces_placed as u64),
        ("TETRIS", g.tetrises as u64),
    ];
    for (k, v) in rows.iter() {
        text(img, cx - 90, y, k, DIM, 2);
        text_num(img, cx + 92, y, *v, TEXT, 2);
        y += 22;
    }
    if g.score > 0 && g.score >= g.best {
        text_center(img, cx, y + 6, "NEW PERSONAL BEST", [255, 220, 120], 2);
    }
    if blink(g, 600.0) {
        text_center(img, cx, by + BOARD_H as i32 - 90, "ENTER TO PLAY AGAIN", ACCENT, 2);
    }
}





