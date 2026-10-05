// Rust + WebAssembly Tetris: the browser only forwards input events and blits
// the framebuffer that Rust owns. No game rules, no canvas text, no JS timers.

"use strict";

const WASM_URL = "./tetris.wasm";
const BEST_KEY = "rustwasm.tetris.best";

// Must match `Action` in src/constants.rs.
const A = {
  Left: 0,
  Right: 1,
  SoftDrop: 2,
  HardDrop: 3,
  RotCW: 4,
  RotCCW: 5,
  Rot180: 6,
  Hold: 7,
  Pause: 8,
  Restart: 9,
  Start: 10,
};

// Must match `ev` in src/constants.rs.
const EV = {
  LINE_CLEAR: 1 << 0,
  TETRIS: 1 << 1,
  LEVEL_UP: 1 << 2,
  LOCK: 1 << 3,
  MOVE: 1 << 4,
  ROTATE: 1 << 5,
  SPAWN: 1 << 6,
  HOLD: 1 << 7,
  HARD_DROP: 1 << 8,
  GAME_OVER: 1 << 9,
  PAUSE: 1 << 10,
  T_SPIN: 1 << 11,
  PERFECT: 1 << 12,
  COMBO: 1 << 13,
};

const PHASE = ["READY", "PLAYING", "PAUSED", "CLEARING", "GAME OVER"];

const KEYMAP = {
  ArrowLeft: A.Left,
  ArrowRight: A.Right,
  ArrowDown: A.SoftDrop,
  Space: A.HardDrop,
  ArrowUp: A.RotCW,
  KeyX: A.RotCW,
  KeyZ: A.RotCCW,
  KeyA: A.Rot180,
  KeyC: A.Hold,
  ControlLeft: A.Hold,
  ControlRight: A.Hold,
  KeyP: A.Pause,
  KeyR: A.Restart,
  Enter: A.Start,
  NumpadEnter: A.Start,
};

const canvas = document.getElementById("screen");
const ctx = canvas.getContext("2d", { alpha: false });
const statusEl = document.getElementById("status");
const soundBtn = document.getElementById("sound");
const bootEl = document.getElementById("boot");

let W = null; // wasm exports
let memory = null; // wasm memory export
let imgData = null; // reused upload buffer for the copy path
let dataBuf = null; // last WebAssembly.Memory buffer we mapped a view of
let soundOn = true;
let ac = null;
let master = null;
let armed = false; // set by the first real gesture (audio autoplay policy)

const held = new Set();

// --------------------------------------------------------------------------
// Boot
// --------------------------------------------------------------------------

async function boot() {
  const imports = {}; // the module imports nothing from JavaScript
  let result;
  try {
    result = await WebAssembly.instantiateStreaming(fetch(WASM_URL), imports);
  } catch (err) {
    // Fall back for servers that send the wrong content type.
    const bytes = await (await fetch(WASM_URL)).arrayBuffer();
    result = await WebAssembly.instantiate(bytes, imports);
  }
  W = result.instance.exports;
  memory = W.memory;

  W.wasm_init((Date.now() ^ 0x5eed1984) >>> 0);
  W.wasm_set_best(readBest() >>> 0);

  canvas.width = W.wasm_fb_w();
  canvas.height = W.wasm_fb_h();
  canvas.addEventListener("pointerdown", () => {
    armed = true;
    ensureAudio();
  });

  bootEl.hidden = true;
  updateStatus();
  requestAnimationFrame(loop);
}

function readBest() {
  try {
    return parseInt(localStorage.getItem(BEST_KEY) || "0", 10) || 0;
  } catch (err) {
    return 0;
  }
}

function writeBest(value) {
  try {
    if (value > readBest()) localStorage.setItem(BEST_KEY, String(value));
  } catch (err) {
    /* private mode: nothing to persist */
  }
}

boot().catch((err) => {
  bootEl.hidden = false;
  bootEl.textContent =
    "Could not load tetris.wasm (" +
    err.message +
    "). Run ./build.sh first, then serve this folder over http://.";
  console.error(err);
});

// --------------------------------------------------------------------------
// Frame loop
// --------------------------------------------------------------------------

let prevTime = 0;
let frameCount = 0;
let fpsAt = 0;
let fps = 0;

function loop(now) {
  const dt = prevTime === 0 ? 16.7 : now - prevTime;
  prevTime = now;

  W.wasm_frame(dt); // step the rules and repaint the framebuffer, in Rust
  blit();

  const events = W.wasm_events();
  if (events) onEvents(events);

  frameCount++;
  if (now - fpsAt >= 500) {
    fps = Math.round((frameCount * 1000) / (now - fpsAt));
    frameCount = 0;
    fpsAt = now;
    updateStatus();
  }
  requestAnimationFrame(loop);
}

// Copy the RGBA8 framebuffer onto the canvas. The pointer is re-read every
// frame: the game allocates inside the module, so the Rust heap - and with it
// the pixel buffer - can move when it grows.
function blit() {
  if (memory.buffer !== dataBuf) {
    dataBuf = memory.buffer;
    imgData = null;
  }
  const w = W.wasm_fb_w();
  const h = W.wasm_fb_h();
  const len = w * h * 4;
  const ptr = W.wasm_fb_ptr() >>> 0;
  try {
    // Zero-copy: hand the canvas a view straight into wasm memory.
    ctx.putImageData(new ImageData(new Uint8ClampedArray(dataBuf, ptr, len), w, h), 0, 0);
  } catch (err) {
    const src = new Uint8Array(dataBuf, ptr, len);
    if (!imgData) imgData = ctx.createImageData(w, h);
    imgData.data.set(src);
    ctx.putImageData(imgData, 0, 0);
  }
}

function updateStatus() {
  if (!W) return;
  statusEl.textContent =
    "phase " +
    PHASE[W.wasm_phase()] +
    "  ·  score " +
    W.wasm_score() +
    "  ·  lines " +
    W.wasm_lines() +
    "  ·  level " +
    W.wasm_level() +
    (fps ? "  ·  " + fps + " fps" : "");
}

// --------------------------------------------------------------------------
// Input
// --------------------------------------------------------------------------

function press(action) {
  ensureAudio();
  if (held.has(action)) return;
  held.add(action);
  W.wasm_key_down(action);
}

function release(action) {
  if (!held.delete(action)) return;
  W.wasm_key_up(action);
}

function togglePause() {
  press(A.Pause);
  release(A.Pause);
}

window.addEventListener("keydown", (e) => {
  if (e.metaKey || e.altKey) return;
  armed = true;
  if (e.code === "KeyM" && !e.repeat) {
    setSound(!soundOn);
    return;
  }
  const action = KEYMAP[e.code];
  if (action === undefined) return;
  e.preventDefault();
  if (!e.repeat) press(action);
});

window.addEventListener("keyup", (e) => {
  const action = KEYMAP[e.code];
  if (action === undefined) return;
  e.preventDefault();
  release(action);
});

function releaseAll() {
  for (const action of held) W.wasm_key_up(action);
  held.clear();
}

// Losing focus must never leave a direction key stuck down.
window.addEventListener("blur", () => {
  releaseAll();
  if (W && W.wasm_phase() === 1) togglePause();
});

document.addEventListener("visibilitychange", () => {
  if (!document.hidden) return;
  releaseAll();
  if (W && W.wasm_phase() === 1) togglePause();
});

// On-screen pad for touch devices (the buttons live in index.html).
for (const el of document.querySelectorAll("[data-act]")) {
  const action = A[el.dataset.act];
  if (action === undefined) continue;
  const down = (e) => {
    e.preventDefault();
    armed = true;
    press(action);
    el.classList.add("on");
  };
  const up = (e) => {
    if (e.cancelable) e.preventDefault();
    release(action);
    el.classList.remove("on");
  };
  el.addEventListener("pointerdown", down);
  el.addEventListener("pointerup", up);
  el.addEventListener("pointercancel", up);
  el.addEventListener("pointerleave", up);
}

// --------------------------------------------------------------------------
// Sound - optional, driven purely by the event bits Rust reports
// --------------------------------------------------------------------------

function ensureAudio() {
  if (!armed || ac || !soundOn) return;
  const Ctor = window.AudioContext || window.webkitAudioContext;
  if (!Ctor) return;
  try {
    ac = new Ctor();
    master = ac.createGain();
    master.gain.value = 0.16;
    master.connect(ac.destination);
  } catch (err) {
    ac = null;
  }
  if (ac && ac.state === "suspended") ac.resume();
}

function tone(freq, dur, type, delay, vol) {
  if (!ac || !soundOn) return;
  const t0 = ac.currentTime + (delay || 0);
  const osc = ac.createOscillator();
  const gain = ac.createGain();
  osc.type = type || "square";
  osc.frequency.setValueAtTime(freq, t0);
  gain.gain.setValueAtTime(0.0001, t0);
  gain.gain.exponentialRampToValueAtTime(vol || 0.4, t0 + 0.006);
  gain.gain.exponentialRampToValueAtTime(0.0001, t0 + dur);
  osc.connect(gain);
  gain.connect(master);
  osc.start(t0);
  osc.stop(t0 + dur + 0.02);
}

function sweep(f0, f1, dur, type) {
  if (!ac || !soundOn) return;
  const t0 = ac.currentTime;
  const osc = ac.createOscillator();
  const gain = ac.createGain();
  osc.type = type || "sawtooth";
  osc.frequency.setValueAtTime(f0, t0);
  osc.frequency.exponentialRampToValueAtTime(Math.max(1, f1), t0 + dur);
  gain.gain.setValueAtTime(0.0001, t0);
  gain.gain.exponentialRampToValueAtTime(0.35, t0 + 0.01);
  gain.gain.exponentialRampToValueAtTime(0.0001, t0 + dur);
  osc.connect(gain);
  gain.connect(master);
  osc.start(t0);
  osc.stop(t0 + dur + 0.02);
}

function chord(freqs, dur, type) {
  freqs.forEach((f, i) => tone(f, dur, type || "triangle", i * 0.05, 0.3));
}

function onEvents(bits) {
  if (bits & EV.MOVE) tone(300, 0.02, "square", 0, 0.12);
  if (bits & EV.ROTATE) tone(480, 0.03, "square", 0, 0.18);
  if (bits & EV.LOCK) tone(150, 0.05, "triangle", 0, 0.28);
  if (bits & EV.HARD_DROP) sweep(420, 90, 0.07, "square");
  if (bits & EV.SPAWN) tone(620, 0.02, "sine", 0, 0.1);
  if (bits & EV.HOLD) tone(700, 0.05, "sine", 0, 0.22);
  if (bits & EV.PAUSE) tone(240, 0.12, "triangle", 0, 0.26);
  if (bits & EV.LINE_CLEAR) {
    const n = bits & EV.TETRIS ? 4 : bits & EV.T_SPIN ? 3 : 2;
    for (let i = 0; i < n; i++) tone(520 + i * 110, 0.08, "square", i * 0.04, 0.26);
  }
  if (bits & EV.T_SPIN) sweep(300, 900, 0.18, "triangle");
  if (bits & EV.TETRIS) chord([523, 659, 784, 1046], 0.28);
  if (bits & EV.COMBO) tone(880, 0.04, "square", 0, 0.16);
  if (bits & EV.PERFECT) chord([659, 880, 1174, 1568], 0.35);
  if (bits & EV.LEVEL_UP) sweep(300, 1200, 0.3, "square");
  if (bits & EV.GAME_OVER) {
    chord([440, 349, 262, 196], 0.5, "sawtooth");
    writeBest(W.wasm_score());
  }
  updateStatus();
}

function setSound(on) {
  soundOn = on;
  if (on) ensureAudio();
  if (master) master.gain.value = on ? 0.16 : 0;
  soundBtn.textContent = on ? "sound: on" : "sound: off";
  soundBtn.classList.toggle("off", !on);
}

soundBtn.addEventListener("click", () => {
  armed = true;
  setSound(!soundOn);
});



