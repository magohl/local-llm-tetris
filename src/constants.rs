//! Playfield + framebuffer geometry and tuning constants.

/// Columns of visible playfield.
pub const COLS: usize = 10;
/// Visible rows of playfield.
pub const ROWS: usize = 20;
/// Hidden buffer rows above the visible field (pieces spawn here).
pub const HIDDEN: usize = 4;
/// Total grid rows including the hidden buffer.
pub const TOTAL_ROWS: usize = ROWS + HIDDEN;

/// Pixel size of one cell.
pub const CELL: u32 = 32;
/// Outer margin of the framebuffer.
pub const MARGIN: u32 = 24;
/// Width of the information panel on the right.
pub const PANEL_W: u32 = 208;
/// Gap between the board and the panel.
pub const GAP: u32 = 24;

/// Board top-left corner in the framebuffer.
pub const BOARD_X: u32 = MARGIN;
pub const BOARD_Y: u32 = MARGIN;
/// Board pixel size.
pub const BOARD_W: u32 = COLS as u32 * CELL;
pub const BOARD_H: u32 = ROWS as u32 * CELL;
/// Panel origin.
pub const PANEL_X: u32 = BOARD_X + BOARD_W + GAP;

/// Framebuffer dimensions (RGBA8).
pub const FB_W: u32 = PANEL_X + PANEL_W + MARGIN;
pub const FB_H: u32 = BOARD_Y + BOARD_H + MARGIN;

// ---- Feel / timing -------------------------------------------------------

/// Delayed Auto Shift: ms a held direction waits before repeating.
pub const DAS: f32 = 133.0;
/// Auto Repeat Rate: ms between repeats once DAS has elapsed.
pub const ARR: f32 = 13.0;
/// Soft drop gravity multiplier.
pub const SOFT_DROP_FACTOR: f32 = 20.0;
/// Lock delay in ms.
pub const LOCK_DELAY: f32 = 500.0;
/// Number of lock-delay resets allowed before the piece forces down.
pub const MAX_LOCK_RESETS: u32 = 15;
/// Length of the line-clear flash.
pub const CLEAR_ANIM_MS: f32 = 300.0;
/// How many upcoming pieces are previewed.
pub const NEXT_PREVIEW: usize = 5;

/// Gravity for a level, in ms per row (Tetris Guideline curve).
pub fn gravity_ms(level: u32) -> f32 {
    if level <= 1 {
        return 1000.0;
    }
    let l = level.min(20) as f32;
    let t = (0.8f32).powf(l - 1.0) * 1000.0;
    // (0.8 - (level-1)*0.007)^(level-1) seconds, clamped to a sane floor.
    let base = (0.8f32 - (l - 1.0) * 0.007).max(0.05);
    let ms = base.powf(l - 1.0) * 1000.0;
    if t.min(ms) < 16.0 {
        16.0
    } else {
        t.min(ms)
    }
}

/// Event bits reported to the host so it can play sounds / react.
pub mod ev {
    pub const LINE_CLEAR: u32 = 1 << 0;
    pub const TETRIS: u32 = 1 << 1;
    pub const LEVEL_UP: u32 = 1 << 2;
    pub const LOCK: u32 = 1 << 3;
    pub const MOVE: u32 = 1 << 4;
    pub const ROTATE: u32 = 1 << 5;
    pub const SPAWN: u32 = 1 << 6;
    pub const HOLD: u32 = 1 << 7;
    pub const HARD_DROP: u32 = 1 << 8;
    pub const GAME_OVER: u32 = 1 << 9;
    pub const PAUSE: u32 = 1 << 10;
    pub const T_SPIN: u32 = 1 << 11;
    pub const PERFECT: u32 = 1 << 12;
    pub const COMBO: u32 = 1 << 13;
    pub const MAX: u32 = 14;
}

/// Game phases, mirrored by the JS glue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Ready,
    Playing,
    Paused,
    Clearing,
    GameOver,
}

/// Player-facing actions, mapped from key codes in JS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Action {
    Left = 0,
    Right = 1,
    SoftDrop = 2,
    HardDrop = 3,
    RotCW = 4,
    RotCCW = 5,
    Rot180 = 6,
    Hold = 7,
    Pause = 8,
    Restart = 9,
    Start = 10,
    Count = 11,
}
