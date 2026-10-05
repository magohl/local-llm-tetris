//! Flat C ABI consumed by `www/main.js`.
//!
//! Deliberately *not* using `wasm-bindgen`: the module only exports plain
//! integers/floats and owns a fixed RGBA framebuffer that the browser reads straight
//! out of `WebAssembly.Memory`, so there is no build-time codegen step to depend on.

use crate::constants::{Phase, FB_H, FB_W};
use crate::game::{action_from_id, Game};
use crate::render::{self, Image};
use std::cell::UnsafeCell;

struct Ctx {
    game: Game,
    img: Image,
}

struct SCell<T>(UnsafeCell<T>);
// WebAssembly instances are single-threaded, so this singleton is never contended.
unsafe impl<T> Sync for SCell<T> {}

static CTX: SCell<Option<Ctx>> = SCell(UnsafeCell::new(None));

unsafe fn ctx() -> &'static mut Ctx {
    let slot = &mut *CTX.0.get();
    if slot.is_none() {
        *slot = Some(Ctx {
            game: Game::new(0x5EED_1984),
            img: Image::new(FB_W, FB_H),
        });
    }
    slot.as_mut().unwrap_unchecked()
}

fn phase_id(p: Phase) -> u32 {
    match p {
        Phase::Ready => 0,
        Phase::Playing => 1,
        Phase::Paused => 2,
        Phase::Clearing => 3,
        Phase::GameOver => 4,
    }
}

#[no_mangle]
pub extern "C" fn wasm_init(seed: u32) {
    let c = unsafe { ctx() };
    c.game = Game::new(seed as u64 | 1);
    render::draw(&c.game, &mut c.img);
}

/// Advance the simulation and re-render one frame.
#[no_mangle]
pub extern "C" fn wasm_frame(dt_ms: f32) {
    let c = unsafe { ctx() };
    c.game.frame(dt_ms);
    render::draw(&c.game, &mut c.img);
}

/// Re-render without advancing time (used after a resize or while paused).
#[no_mangle]
pub extern "C" fn wasm_render() {
    let c = unsafe { ctx() };
    render::draw(&c.game, &mut c.img);
}

#[no_mangle]
pub extern "C" fn wasm_fb_ptr() -> u32 {
    unsafe { ctx().img.buf.as_ptr() as u32 }
}

#[no_mangle]
pub extern "C" fn wasm_fb_w() -> u32 {
    FB_W
}

#[no_mangle]
pub extern "C" fn wasm_fb_h() -> u32 {
    FB_H
}

#[no_mangle]
pub extern "C" fn wasm_key_down(action: u32) {
    if let Some(a) = action_from_id(action) {
        unsafe { ctx().game.key_down(a) };
    }
}

#[no_mangle]
pub extern "C" fn wasm_key_up(action: u32) {
    if let Some(a) = action_from_id(action) {
        unsafe { ctx().game.key_up(a) };
    }
}

/// Event bits raised since the last call (also clears them).
#[no_mangle]
pub extern "C" fn wasm_events() -> u32 {
    let c = unsafe { ctx() };
    let e = c.game.events;
    c.game.events = 0;
    e
}

#[no_mangle]
pub extern "C" fn wasm_phase() -> u32 {
    unsafe { phase_id(ctx().game.phase) }
}

#[no_mangle]
pub extern "C" fn wasm_score() -> u32 {
    unsafe { ctx().game.score }
}

#[no_mangle]
pub extern "C" fn wasm_lines() -> u32 {
    unsafe { ctx().game.lines }
}

#[no_mangle]
pub extern "C" fn wasm_level() -> u32 {
    unsafe { ctx().game.level }
}

#[no_mangle]
pub extern "C" fn wasm_piece() -> u32 {
    unsafe { ctx().game.piece.kind as u32 + 1 }
}

#[no_mangle]
pub extern "C" fn wasm_hold() -> u32 {
    let c = unsafe { ctx() };
    if c.game.has_hold {
        c.game.hold as u32 + 1
    } else {
        0
    }
}

/// Value of a grid cell (0 empty, 1..=7 block colour, 255 out of range).
#[no_mangle]
pub extern "C" fn wasm_cell(x: u32, y: u32) -> u32 {
    let c = unsafe { ctx() };
    let v = c.game.board.get(x as i32 + 0, y as i32 + 0);
    if v < 0 {
        255
    } else {
        v as u32
    }
}

/// Number of blocks currently resting on the visible field.
#[no_mangle]
pub extern "C" fn wasm_blocks() -> u32 {
    unsafe { ctx().game.board.visible_blocks() as u32 }
}

#[no_mangle]
pub extern "C" fn wasm_set_best(value: u32) {
    unsafe { ctx().game.best = value };
}

#[no_mangle]
pub extern "C" fn wasm_restart() {
    unsafe { ctx().game.start() };
}

/// Deterministic spawn control for automated tests.
#[no_mangle]
pub extern "C" fn wasm_force_next(kind: u32) {
    let c = unsafe { ctx() };
    if kind < 7 {
        c.game.force_next(crate::pieces::Kind::from_u8(kind as u8));
    }
}

/// Start a game immediately (skips the title screen).
#[no_mangle]
pub extern "C" fn wasm_start() {
    unsafe { ctx().game.start() };
}
