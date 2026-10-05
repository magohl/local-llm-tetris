//! Tetris-style game in pure Rust, compiled to WebAssembly.
//!
//! The crate is target-agnostic: all game rules and the software renderer live in
//! portable Rust (`board`, `pieces`, `game`, `render`, `font`), which means the exact
//! same code is exercised by `cargo test` on the host. The `wasm` module adds the flat
//! C ABI that the browser glue (`www/main.js`) talks to.

#![allow(clippy::too_many_arguments)]

pub mod board;
pub mod constants;
pub mod font;
pub mod game;
pub mod pieces;
pub mod render;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use constants::*;
pub use game::Game;
