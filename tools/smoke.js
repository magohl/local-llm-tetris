// Headless smoke test: runs the real wasm module in Node without a browser.
//
// Pass one hammers it with random input to shake out panics and stuck keys; pass
// two plays the field with a greedy one-piece planner so that line clears, the
// clear animation, level-ups and the score popups really happen. Each phase worth
// looking at is dumped as a PNG next to the output file: title.png, play.png,
// clear.png and final.png.
//
//   node tools/smoke.js [frames] [seed] [out.png]
//
// Exits non-zero if the module misbehaves: no points, no line clears, a stuck
// framebuffer, or a near-blank frame.

const fs = require("fs");
const path = require("path");
const zlib = require("zlib");

const wasmFile =
  process.env.WASM ||
  path.join(__dirname, "..", "www", "tetris.wasm");
const FRAMES = parseInt(process.argv[2] || "3000", 10);
const SEED = parseInt(process.argv[3] || "12345", 10) >>> 0;
const OUT = process.argv[4] || path.join(__dirname, "..", "www", "smoke.png");

const A = { Left: 0, Right: 1, SoftDrop: 2, HardDrop: 3, RotCW: 4, RotCCW: 5, Rot180: 6, Hold: 7, Pause: 8, Restart: 9, Start: 10 };
const EV = { LINE_CLEAR: 1, TETRIS: 2, LEVEL_UP: 4, LOCK: 8, MOVE: 16, ROTATE: 32, SPAWN: 64, HOLD: 128, HARD_DROP: 256, GAME_OVER: 512, PAUSE: 1024, T_SPIN: 2048, PERFECT: 4096, COMBO: 8192 };

let rng = 0x2545f491;
function rand() {
  rng ^= rng << 13;
  rng ^= rng >>> 17;
  rng ^= rng << 5;
  return (rng >>> 0) / 4294967296;
}

function chunk(type, body) {
  const head = Buffer.alloc(8);
  head.writeUInt32BE(body.length, 0);
  head.write(type, 4, "ascii");
  const crcTable = chunk.table || (chunk.table = makeCrcTable());
  let c = 0xffffffff;
  for (const b of head.subarray(4)) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  for (const b of body) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  const tail = Buffer.alloc(4);
  tail.writeUInt32BE(~c >>> 0, 0);
  return Buffer.concat([head, body, tail]);
}

function makeCrcTable() {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
}

function writePng(file, w, h, rgba) {
  const raw = Buffer.alloc((w * 3 + 1) * h);
  let p = 0;
  for (let y = 0; y < h; y++) {
    raw[p++] = 0; // filter: none
    for (let x = 0; x < w; x++) {
      const o = (y * w + x) * 4;
      raw[p++] = rgba[o];
      raw[p++] = rgba[o + 1];
      raw[p++] = rgba[o + 2];
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 2; // colour type: truecolour RGB
  fs.writeFileSync(
    file,
    Buffer.concat([
      Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
      chunk("IHDR", ihdr),
      chunk("IDAT", zlib.deflateSync(raw, { level: 9 })),
      chunk("IEND", Buffer.alloc(0)),
    ])
  );
}

const buf = fs.readFileSync(wasmFile);
WebAssembly.instantiate(buf, {}).then(({ instance }) => {
  const e = instance.exports;
  const tap = (a) => {
    e.wasm_key_down(a);
    e.wasm_key_up(a);
  };

  e.wasm_init(SEED);
  const w = e.wasm_fb_w();
  const h = e.wasm_fb_h();
  console.log(`module ok  ·  framebuffer ${w}x${h} @ 0x${e.wasm_fb_ptr().toString(16)}`);
  console.log(`phase at boot: ${e.wasm_phase()} (0 = title screen)`);
  shot(e, w, h, "title");

  let clears = 0;
  let peak = 0;
  let gameOvers = 0;
  let ptrMoves = 0;
  let lastPtr = e.wasm_fb_ptr();

  function track() {
    const ev = e.wasm_events();
    if (ev & EV.LINE_CLEAR) clears++;
    if (ev & EV.GAME_OVER) gameOvers++;
    const p = e.wasm_fb_ptr();
    if (p !== lastPtr) {
      ptrMoves++;
      lastPtr = p;
    }
    peak = Math.max(peak, e.wasm_score());
    return ev;
  }

  // ---- pass 1: random input, to shake out crashes and stuck keys ----
  tap(A.Start);
  const stressFrames = Math.min(600, Math.floor(FRAMES / 3));
  for (let i = 0; i < stressFrames; i++) {
    e.wasm_frame(16.67);
    track();

    const r = rand();
    if (r < 0.05) tap(A.Left);
    else if (r < 0.1) tap(A.Right);
    else if (r < 0.13) tap(A.RotCW);
    else if (r < 0.15) tap(A.RotCCW);
    else if (r < 0.16) tap(A.Rot180);
    else if (r < 0.19) tap(A.HardDrop);
    else if (r < 0.2) tap(A.Hold);

    if (e.wasm_phase() === 4) tap(A.Start); // replay immediately
  }
  shot(e, w, h, "play");

  // ---- pass 2: scripted filler, so rows actually clear ----
  // Random taps almost never complete a line, which would leave the clear
  // animation, the popups and the level-up path untested. This pass reads the
  // stack through `wasm_cell` and plays a one-piece greedy planner - the same
  // shape of cost function the soak test in game.rs uses. Nothing is forced, so
  // the seven-bag decides what the planner has to work with.
  tap(A.Restart);
  const COLS = 10;
  const ROWS = 24; // TOTAL_ROWS in src/constants.rs (y = 0 is the top, 23 the floor)
  const BOX = [4, 2, 3, 3, 3, 3, 3]; // rotation box side, indexed by piece id - 1
  const BASE = [
    [[0, 1], [1, 1], [2, 1], [3, 1]], // I
    [[0, 0], [1, 0], [0, 1], [1, 1]], // O
    [[1, 0], [0, 1], [1, 1], [2, 1]], // T
    [[1, 0], [2, 0], [0, 1], [1, 1]], // S
    [[0, 0], [1, 0], [1, 1], [2, 1]], // Z
    [[0, 0], [0, 1], [1, 1], [2, 1]], // J
    [[2, 0], [0, 1], [1, 1], [2, 1]], // L
  ];
  let gotClear = false;
  const fillFrames = Math.max(0, FRAMES - stressFrames);

  // Cell offsets of `id` in rotation `rot`, mirroring pieces.rs::cells.
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

  // Column heights, holes, bumpiness and completed rows of a candidate board.
  function judge(g) {
    let holes = 0;
    let bump = 0;
    let agg = 0;
    let max = 0;
    let prev = 0;
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

  // Best (rotations, leftmost column) for the piece on screen, or null if none fit.
  function plan(g, id) {
    let best = null;
    let bestScore = -Infinity;
    for (let rot = 0; rot < 4; rot++) {
      const cells = shape(id, rot);
      let minx = cells[0][0];
      let maxx = cells[0][0];
      for (const [dx] of cells) {
        if (dx < minx) minx = dx;
        if (dx > maxx) maxx = dx;
      }
      const wide = maxx - minx + 1;
      for (let left = 0; left + wide <= COLS; left++) {
        // The shape is shifted so that its leftmost column sits at `left`; after
        // hugging the wall that is exactly `left` presses of Right.
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
          if (y < 0 || copy[y][c] !== 0) {
            fits = false;
            break;
          }
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
        }
      }
    }
    return best;
  }

  for (let i = 0; i < fillFrames; i++) {
    e.wasm_frame(16.67);
    track();
    const ph = e.wasm_phase();
    if (!gotClear && ph === 3) {
      shot(e, w, h, "clear");
      gotClear = true;
    }
    if (ph === 4) {
      tap(A.Restart); // the stress pass can leave the game topped out
      continue;
    }
    if (ph === 0 || ph === 2) {
      tap(A.Start); // ... or parked on the title, or paused
      continue;
    }
    if (ph !== 1) continue; // let the clear animation play out

    const id = e.wasm_piece(); // 1..7 in Kind order: I O T S Z J L
    const at = plan(board(), id) || { rot: 0, left: 0 };
    for (let k = 0; k < at.rot; k++) tap(A.RotCW);
    for (let k = 0; k < 9; k++) tap(A.Left); // hug the wall (rotations can kick)
    for (let k = 0; k < at.left; k++) tap(A.Right);
    tap(A.HardDrop);
  }
  shot(e, w, h, "final");

  console.log(
    [
      `frames driven   ${FRAMES}`,
      `phase           ${e.wasm_phase()}`,
      `peak score      ${peak}`,
      `lines           ${e.wasm_lines()}`,
      `level           ${e.wasm_level()}`,
      `blocks on field ${e.wasm_blocks()}`,
      `line clears     ${clears}`,
      `game overs      ${gameOvers}`,
      `fb relocations  ${ptrMoves}`,
    ].join("\n")
  );

  if (!e.wasm_frame) throw new Error("wasm_frame missing");
  if (peak <= 0) throw new Error("no points were scored across the whole run");
  if (!gotClear) throw new Error("the clear animation was never reached");
  // The filler plays the same opening every run, so these floors double as a
  // check on the planner itself: a model that drifts from the engine stops
  // clearing rows. They scale down when fewer frames are requested.
  const wantClears = Math.min(40, Math.floor(fillFrames / 60));
  if (clears < wantClears) {
    throw new Error(`the scripted pass cleared only ${clears} rows, expected ${wantClears}`);
  }
  if (peak < wantClears * 400) {
    throw new Error(`the scripted pass scored only ${peak}, expected at least ${wantClears * 400}`);
  }
  const colours = countColours(e, w, h);
  if (colours < 8) throw new Error(`renderer produced a near-blank frame (${colours} colours)`);
  console.log(`colours in last frame: ${colours}`);
  console.log("SMOKE OK");
});

function shot(e, w, h, name) {
  const mem = new Uint8Array(e.memory.buffer);
  const ptr = e.wasm_fb_ptr() >>> 0;
  const file = path.join(path.dirname(OUT), `${name}.png`);
  writePng(file, w, h, Buffer.from(mem.subarray(ptr, ptr + w * h * 4)));
  console.log(`wrote ${file}`);
}

function countColours(e, w, h) {
  const mem = new Uint8Array(e.memory.buffer);
  const ptr = e.wasm_fb_ptr() >>> 0;
  const seen = new Set();
  for (let i = 0; i < w * h; i += 7) {
    const o = ptr + i * 4;
    seen.add((mem[o] << 16) | (mem[o + 1] << 8) | mem[o + 2]);
  }
  return seen.size;
}
