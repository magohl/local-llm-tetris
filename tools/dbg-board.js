#!/usr/bin/env node
// Debug companion to smoke.js: plays the field with the same greedy planner and
// checks its board model against the engine after every lock, then dumps the
// stack when a game ends. "model mismatches: 0" means the planner sees the board
// exactly as game.rs does; anything else means the tool and the engine disagree
// on coordinates, and the screenshots in smoke.js cannot be trusted either.
//
//   node tools/dbg-board.js
//   PLACE_LOG=1     ... log every placement the planner makes
//   CHOOSE_LOG=1    ... log the score of each candidate placement
"use strict";
const fs = require("fs");
const path = require("path");
const ROOT = path.resolve(__dirname, "..");
const wasmBytes = fs.readFileSync(path.join(ROOT, "www", "tetris.wasm"));

const A = { Left: 0, Right: 1, SoftDrop: 2, HardDrop: 3, RotCW: 4, RotCCW: 5, Rot180: 6, Hold: 7, Pause: 8, Restart: 9, Start: 10 };
const PH = ["READY", "PLAYING", "PAUSED", "CLEAR", "OVER"];
let e;

function dump(title) {
  const rows = [];
  for (let y = 23; y >= 0; y--) {
    let s = String(y).padStart(2) + " ";
    for (let x = 0; x < 10; x++) {
      const v = e.wasm_cell(x, y);
      s += v === 0 ? "." : v === 255 ? "!" : "01234567"[v];
    }
    rows.push(s);
  }
  console.log("=== " + title + "  phase=" + PH[e.wasm_phase()] + " score=" + e.wasm_score() + " lines=" + e.wasm_lines() + " blocks=" + e.wasm_blocks());
  console.log(rows.join("\n"));
}

function tap(a) { e.wasm_key_down(a); e.wasm_key_up(a); }

const COLS = 10;
const ROWS = 24;
const BOX = [4, 2, 3, 3, 3, 3, 3];
const BASE = [
  [[0, 1], [1, 1], [2, 1], [3, 1]], // I
  [[0, 0], [1, 0], [0, 1], [1, 1]], // O
  [[1, 0], [0, 1], [1, 1], [2, 1]], // T
  [[1, 0], [2, 0], [0, 1], [1, 1]], // S
  [[0, 0], [1, 0], [1, 1], [2, 1]], // Z
  [[0, 0], [0, 1], [1, 1], [2, 1]], // J
  [[2, 0], [0, 1], [1, 1], [2, 1]], // L
];
function shape(id, rot) {
  const s = BOX[id - 1];
  return BASE[id - 1].map(([x, y]) => {
    switch (rot & 3) {
      case 1: return [s - 1 - y, x];
      case 2: return [s - 1 - x, s - 1 - y];
      case 3: return [y, s - 1 - x];
      default: return [x, y];
    }
  });
}
function board() {
  const g = [];
  for (let y = 0; y < ROWS; y++) {
    const row = [];
    for (let x = 0; x < COLS; x++) row.push(e.wasm_cell(x, y));
    g.push(row);
  }
  return g;
}
function judge(g) {
  let holes = 0, bump = 0, agg = 0, max = 0, prev = 0;
  for (let x = 0; x < COLS; x++) {
    let y = 0;
    while (y < ROWS && g[y][x] === 0) y++;
    const h = ROWS - y;
    for (let r = y + 1; r < ROWS; r++) if (g[r][x] === 0) holes++;
    agg += h;
    bump += Math.abs(h - prev);
    prev = h;
    if (h > max) max = h;
  }
  let clears = 0;
  for (let y = 0; y < ROWS; y++) if (g[y].every((v) => v !== 0)) clears++;
  return { holes, bump, agg, max, clears };
}
function plan(g, id) {
  let best = null, bestScore = -Infinity, bestAt = null;
  const log = [];
  for (let rot = 0; rot < 4; rot++) {
    const cells = shape(id, rot);
    let minx = cells[0][0], maxx = cells[0][0];
    for (const [dx] of cells) { if (dx < minx) minx = dx; if (dx > maxx) maxx = dx; }
    const wide = maxx - minx + 1;
    for (let left = 0; left + wide <= COLS; left++) {
      const at = cells.map(([dx, dy]) => [left + dx - minx, dy]);
      let by = ROWS;
      for (const [c, dy] of at) {
        let y = 0;
        while (y < ROWS && g[y][c] === 0) y++;
        by = Math.min(by, y - 1 - dy);
      }
      const copy = g.map((row) => row.slice());
      let fits = true;
      for (const [c, dy] of at) {
        const y = by + dy;
        if (y < 0 || copy[y][c] !== 0) { fits = false; break; }
        copy[y][c] = id;
      }
      if (!fits) continue;
      const kept = copy.filter((row) => !row.every((v) => v !== 0));
      while (kept.length < ROWS) kept.unshift(new Array(COLS).fill(0));
      const j = judge(kept);
      const score = j.clears * 5000 - (j.agg * 15 + j.holes * 250 + j.bump * 30 + j.max * 15);
      if (score > bestScore) {
        bestScore = score;
        best = { rot, left };
        bestAt = at.map(([c, dy]) => [c, by + dy]);
      }
      if (process.env.CHOOSE_LOG) log.push("r" + rot + "@" + left + ":" + score);
    }
  }
  if (process.env.CHOOSE_LOG) console.log("  id=" + id + " -> r" + best.rot + "@x" + best.left + " | " + log.slice(0, 12).join(" "));
  return best ? Object.assign(best, { cells: bestAt }) : { rot: 0, left: 0, cells: [] };
}

// Cells the engine actually filled in, i.e. the board delta of one lock.
function delta(a, b) {
  const out = [];
  for (let y = 0; y < ROWS; y++) {
    for (let x = 0; x < COLS; x++) {
      if (a[y][x] === 0 && b[y][x] !== 0) out.push(x + "," + y);
    }
  }
  return out;
}

WebAssembly.instantiate(wasmBytes, {}).then((r) => {
  e = r.instance.exports;
  e.wasm_init(2026);
  tap(A.Restart);
  e.wasm_frame(16.7);
  let pieces = 0;
  let clears = 0;
  let games = 0;
  let mismatch = 0;
  for (let i = 0; i < 2400; i++) {
    e.wasm_frame(16.7);
    const ev = e.wasm_events();
    if (ev & (1 << 0)) clears++;
    const ph = e.wasm_phase();
    if (ph === 4) {
      games++;
      if (games <= 2) {
        console.log("### GAME OVER " + games + " after " + pieces + " pieces, " + clears + " total clears");
        dump("board");
      }
      tap(A.Start);
      pieces = 0;
      continue;
    }
    if (ph !== 1) continue;
    const id = e.wasm_piece();
    const before = board();
    const at = plan(before, id);
    for (let k = 0; k < at.rot; k++) tap(A.RotCW);
    for (let k = 0; k < 9; k++) tap(A.Left);
    for (let k = 0; k < at.left; k++) tap(A.Right);
    tap(A.HardDrop);
    if (process.env.PLACE_LOG && pieces < 60) {
      console.log("piece " + pieces + " id=" + id + " r" + at.rot + " left=" + at.left);
    }
    const after = board();
    const got = delta(before, after);
    const want = at.cells.map(([c, r]) => c + "," + r).sort().join(" ");
    if (got.slice().sort().join(" ") !== want) {
      mismatch++;
      if (mismatch < 8) console.log("MISMATCH piece " + pieces + " id=" + id + " r" + at.rot + " left=" + at.left + "\n  want: " + want + "\n  got:  " + got.sort().join(" "));
    }
    pieces++;
  }
  console.log("model mismatches: " + mismatch + " / " + pieces + " pieces");
});

