//! Game rules: piece flow, input with DAS/ARR, gravity, lock delay,
//! line clears, T-spins, back-to-back, combos and scoring.

use crate::board::Board;
use crate::constants::*;
use crate::pieces::{kicks, Kind, Piece};

const N_ACTIONS: usize = Action::Count as usize;
/// Most rows a single tetromino can complete.
const MAX_CLEAR_ROWS: usize = 4;

/// A short-lived popup such as "TETRIS!" or "+1200".
#[derive(Clone)]
pub struct Msg {
    pub text: [u8; 24],
    pub len: usize,
    pub timer: f32,
    pub total: f32,
    pub big: bool,
}

impl Msg {
    fn new() -> Msg {
        Msg { text: [0; 24], len: 0, timer: 0.0, total: 0.0, big: false }
    }
}

/// Everything needed to play: the model and the feel knobs.
pub struct Game {
    pub board: Board,
    pub piece: Piece,
    pub phase: Phase,

    // Piece flow
    pub queue: [Kind; 24],
    pub qlen: usize,
    pub hold: Kind,
    pub has_hold: bool,
    pub can_hold: bool,

    // Score / stats
    pub score: u32,
    pub best: u32,
    pub lines: u32,
    pub level: u32,
    pub combo: i32,
    pub max_combo: i32,
    pub b2b: i32,
    pub best_b2b: i32,
    pub pieces_placed: u32,
    pub tetrises: u32,
    pub tspins: u32,
    pub holds_used: u32,
    pub time_ms: f32,
    /// Runs in every phase (including Ready/Paused) so the UI can animate.
    pub ui_time: f32,

    // Timers
    pub gravity_acc: f32,
    pub lock_timer: f32,
    pub lock_resets: u32,
    pub grounded: bool,
    pub lowest_y: i32,
    pub clear_rows: [usize; MAX_CLEAR_ROWS],
    pub n_clear: usize,
    pub clear_timer: f32,

    // Input state
    pub held: [bool; N_ACTIONS],
    pub das_dir: i32,
    pub das_timer: f32,
    pub arr_acc: f32,

    // Feedback for the renderer / host
    pub events: u32,
    pub last_rotate: bool,
    pub kick_used: usize,
    pub msg: Msg,
    pub sub_msg: Msg,
    pub flash: f32,
    pub shake: f32,
    pub spawn_anim: f32,

    rng: u64,
}

impl Game {
    pub fn new(seed: u64) -> Game {
        let mut g = Game {
            board: Board::new(),
            piece: Piece::new(Kind::T),
            phase: Phase::Ready,
            queue: [Kind::I; 24],
            qlen: 0,
            hold: Kind::I,
            has_hold: false,
            can_hold: true,
            score: 0,
            best: 0,
            lines: 0,
            level: 1,
            combo: -1,
            max_combo: 0,
            b2b: -1,
            best_b2b: 0,
            pieces_placed: 0,
            tetrises: 0,
            tspins: 0,
            holds_used: 0,
            time_ms: 0.0,
            ui_time: 0.0,
            gravity_acc: 0.0,
            lock_timer: 0.0,
            lock_resets: 0,
            grounded: false,
            lowest_y: 0,
            clear_rows: [0; MAX_CLEAR_ROWS],
            n_clear: 0,
            clear_timer: 0.0,
            held: [false; N_ACTIONS],
            das_dir: 0,
            das_timer: 0.0,
            arr_acc: 0.0,
            events: 0,
            last_rotate: false,
            kick_used: 0,
            msg: Msg::new(),
            sub_msg: Msg::new(),
            flash: 0.0,
            shake: 0.0,
            spawn_anim: 0.0,
            rng: if seed == 0 { 0x9E3779B97F4A7C15 } else { seed },
        };
        g.refill();
        g
    }

    /// Start (or restart) a brand new game.
    pub fn start(&mut self) {
        self.board = Board::new();
        self.qlen = 0;
        self.refill();
        self.has_hold = false;
        self.can_hold = true;
        self.score = 0;
        self.lines = 0;
        self.level = 1;
        self.combo = -1;
        self.max_combo = 0;
        self.b2b = -1;
        self.best_b2b = 0;
        self.pieces_placed = 0;
        self.tetrises = 0;
        self.tspins = 0;
        self.holds_used = 0;
        self.time_ms = 0.0;
        self.gravity_acc = 0.0;
        self.lock_timer = 0.0;
        self.lock_resets = 0;
        self.grounded = false;
        self.n_clear = 0;
        self.clear_timer = 0.0;
        self.held = [false; N_ACTIONS];
        self.das_dir = 0;
        self.das_timer = 0.0;
        self.arr_acc = 0.0;
        self.last_rotate = false;
        self.msg = Msg::new();
        self.sub_msg = Msg::new();
        self.flash = 0.0;
        self.shake = 0.0;
        self.spawn_anim = 0.0;
        self.phase = Phase::Playing;
        self.spawn_next();
    }

    /// xorshift64* — tiny, fast and good enough for dropping blocks.
    fn random(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    /// Append shuffled 7-bags while there is room.
    fn refill(&mut self) {
        while self.qlen + 7 <= self.queue.len() {
            let start = self.qlen;
            for (i, k) in Kind::ALL.iter().enumerate() {
                self.queue[start + i] = *k;
            }
            self.qlen += 7;
            for i in (1..7).rev() {
                let j = (self.random() % (i as u64 + 1)) as usize;
                self.queue.swap(start + i, start + j);
            }
        }
    }

    /// Pop one kind off the queue, keeping it topped up.
    fn take_next(&mut self) -> Kind {
        if self.qlen < 8 {
            self.refill();
        }
        let k = self.queue[0];
        for i in 0..self.qlen - 1 {
            self.queue[i] = self.queue[i + 1];
        }
        self.qlen -= 1;
        self.refill();
        k
    }

    /// Upcoming pieces for the preview panel.
    pub fn preview(&self, n: usize) -> Vec<Kind> {
        self.queue.iter().take(n.min(self.qlen)).copied().collect()
    }

    /// Force the next piece (used by tests and by a debug key).
    pub fn force_next(&mut self, kind: Kind) {
        self.queue[0] = kind;
    }
}


/// Read a `Msg` payload back out as a string.
pub fn msg_text(m: &Msg) -> &str {
    let end = m.len.min(m.text.len());
    std::str::from_utf8(&m.text[..end]).unwrap_or("")
}

impl Game {
    // ---- Piece lifecycle --------------------------------------------------

    /// Place a new piece at the top. Handles the "spawn blocked" top-out.
    pub fn spawn(&mut self, kind: Kind) {
        self.piece = Piece::new(kind);
        if self.board.collides(&self.piece) {
            self.piece.y -= 1;
            if self.board.collides(&self.piece) {
                self.game_over("TOPPED OUT");
                return;
            }
        }
        self.gravity_acc = 0.0;
        self.lock_timer = 0.0;
        self.lock_resets = 0;
        self.grounded = false;
        self.lowest_y = self.piece.y;
        self.last_rotate = false;
        self.kick_used = 0;
        self.spawn_anim = 90.0;
        if self.phase != Phase::GameOver {
            self.phase = Phase::Playing;
        }
        self.events |= ev::SPAWN;
    }

    /// Pull the next piece from the queue and put it on the field.
    fn spawn_next(&mut self) {
        let k = self.take_next();
        self.spawn(k);
    }

    fn game_over(&mut self, why: &str) {
        if self.phase == Phase::GameOver {
            return;
        }
        self.phase = Phase::GameOver;
        self.events |= ev::GAME_OVER;
        if self.score > self.best {
            self.best = self.score;
        }
        self.set_msg(why, 1.0e9, true);
        self.shake = 240.0;
    }

    /// Swap the active piece with the hold slot.
    pub fn hold(&mut self) {
        if !self.can_hold || self.phase != Phase::Playing {
            return;
        }
        let current = self.piece.kind;
        self.events |= ev::HOLD;
        self.holds_used += 1;
        if self.has_hold {
            let swapped = self.hold;
            self.hold = current;
            self.spawn(swapped);
        } else {
            self.hold = current;
            self.has_hold = true;
            self.spawn_next();
        }
        self.can_hold = false;
    }

    // ---- Movement ---------------------------------------------------------

    fn on_piece_moved(&mut self) {
        self.last_rotate = false;
        if self.grounded && self.lock_resets < MAX_LOCK_RESETS {
            self.lock_timer = 0.0;
            self.lock_resets += 1;
        }
    }

    /// Shift the piece horizontally; false when blocked.
    pub fn try_move(&mut self, dx: i32) -> bool {
        if self.phase != Phase::Playing {
            return false;
        }
        let next = self.piece.translated(dx, 0);
        if self.board.collides(&next) {
            return false;
        }
        self.piece = next;
        self.events |= ev::MOVE;
        self.on_piece_moved();
        true
    }

    /// One gravity step; false when the piece cannot descend.
    pub fn step_down(&mut self, scoring: bool) -> bool {
        if self.phase != Phase::Playing {
            return false;
        }
        let next = self.piece.translated(0, 1);
        if self.board.collides(&next) {
            self.grounded = true;
            return false;
        }
        self.piece = next;
        if scoring {
            self.score += 1;
        }
        if self.piece.y > self.lowest_y {
            self.lowest_y = self.piece.y;
            self.lock_timer = 0.0;
            self.lock_resets = 0;
            self.grounded = false;
        }
        true
    }

    /// Rotate by `dr` quarter turns (1 = CW, 3 = CCW, 2 = 180), trying SRS kicks.
    pub fn rotate(&mut self, dr: u32) -> bool {
        if self.phase != Phase::Playing {
            return false;
        }
        let from = self.piece.rot;
        let to = (from + dr) & 3;
        for (i, &(dx, dy)) in kicks(self.piece.kind, from, to).iter().enumerate() {
            let next = Piece {
                kind: self.piece.kind,
                rot: to,
                x: self.piece.x + dx,
                y: self.piece.y + dy,
            };
            if !self.board.collides(&next) {
                self.piece = next;
                self.kick_used = i;
                self.last_rotate = true;
                self.events |= ev::ROTATE;
                self.on_piece_moved();
                return true;
            }
        }
        false
    }

    /// Drop to the bottom and lock immediately; returns the distance fallen.
    pub fn hard_drop(&mut self) -> i32 {
        if self.phase != Phase::Playing {
            return 0;
        }
        let mut dist = 0i32;
        while self.step_down(false) {
            dist += 1;
        }
        self.score += (dist * 2) as u32;
        self.events |= ev::HARD_DROP;
        self.shake = (self.shake + dist.min(20) as f32 * 0.5).min(10.0);
        self.lock();
        dist
    }

    /// Row the current piece would land on (for the ghost block).
    pub fn ghost_y(&self) -> i32 {
        let mut p = self.piece;
        while !self.board.collides(&p.translated(0, 1)) {
            p = p.translated(0, 1);
        }
        p.y
    }
}


/// Map a numeric action id (as sent by the JS glue) to an `Action`.
pub fn action_from_id(id: u32) -> Option<Action> {
    if id >= Action::Count as u32 {
        return None;
    }
    Some(match id {
        0 => Action::Left,
        1 => Action::Right,
        2 => Action::SoftDrop,
        3 => Action::HardDrop,
        4 => Action::RotCW,
        5 => Action::RotCCW,
        6 => Action::Rot180,
        7 => Action::Hold,
        8 => Action::Pause,
        9 => Action::Restart,
        _ => Action::Start,
    })
}

impl Game {
    // ---- Input ------------------------------------------------------------

    pub fn key_down(&mut self, a: Action) {
        let idx = a as usize;
        if idx >= N_ACTIONS {
            return;
        }
        let was = self.held[idx];
        self.held[idx] = true;
        if was {
            return; // ignore OS auto-repeat
        }
        match a {
            Action::Start => match self.phase {
                Phase::Ready | Phase::GameOver => self.start(),
                Phase::Paused => self.set_pause(false),
                _ => {}
            },
            Action::Restart => self.start(),
            Action::Pause => match self.phase {
                Phase::Playing => self.set_pause(true),
                Phase::Paused => self.set_pause(false),
                _ => {}
            },
            Action::Left if self.phase == Phase::Playing => {
                self.das_dir = -1;
                self.das_timer = 0.0;
                self.arr_acc = 0.0;
                self.try_move(-1);
            }
            Action::Right if self.phase == Phase::Playing => {
                self.das_dir = 1;
                self.das_timer = 0.0;
                self.arr_acc = 0.0;
                self.try_move(1);
            }
            Action::SoftDrop if self.phase == Phase::Playing => {
                self.step_down(true);
                self.gravity_acc = 0.0;
            }
            Action::HardDrop if self.phase == Phase::Playing => {
                self.hard_drop();
            }
            Action::RotCW if self.phase == Phase::Playing => {
                self.rotate(1);
            }
            Action::RotCCW if self.phase == Phase::Playing => {
                self.rotate(3);
            }
            Action::Rot180 if self.phase == Phase::Playing => {
                self.rotate(2);
            }
            Action::Hold if self.phase == Phase::Playing => {
                self.hold();
            }
            _ => {}
        }
    }

    pub fn key_up(&mut self, a: Action) {
        let idx = a as usize;
        if idx >= N_ACTIONS {
            return;
        }
        self.held[idx] = false;
        if a == Action::Left && self.das_dir == -1 {
            self.das_dir = if self.held[Action::Right as usize] { 1 } else { 0 };
            self.das_timer = 0.0;
            self.arr_acc = 0.0;
        }
        if a == Action::Right && self.das_dir == 1 {
            self.das_dir = if self.held[Action::Left as usize] { -1 } else { 0 };
            self.das_timer = 0.0;
            self.arr_acc = 0.0;
        }
    }

    pub fn set_pause(&mut self, paused: bool) {
        if paused && self.phase == Phase::Playing {
            self.phase = Phase::Paused;
            self.events |= ev::PAUSE;
        } else if !paused && self.phase == Phase::Paused {
            self.phase = Phase::Playing;
            self.gravity_acc = 0.0;
            self.events |= ev::PAUSE;
        }
    }

    fn tick_input(&mut self, dt: f32) {
        let left = self.held[Action::Left as usize];
        let right = self.held[Action::Right as usize];
        let dir = if left == right {
            0
        } else if left {
            -1
        } else {
            1
        };
        if dir == 0 {
            self.das_dir = 0;
            self.das_timer = 0.0;
            self.arr_acc = 0.0;
            return;
        }
        if dir != self.das_dir {
            self.das_dir = dir;
            self.das_timer = 0.0;
            self.arr_acc = 0.0;
            self.try_move(dir);
            return;
        }
        self.das_timer += dt;
        if self.das_timer < DAS {
            return;
        }
        self.arr_acc += dt;
        let mut guard = 0;
        while self.arr_acc >= ARR && guard < 30 {
            guard += 1;
            self.arr_acc -= ARR;
            if !self.try_move(dir) {
                self.arr_acc = 0.0;
                break;
            }
        }
    }
}

impl Game {
    // ---- Main loop --------------------------------------------------------

    /// Advance the simulation by `dt` milliseconds (clamped for tab-switch safety).
    pub fn frame(&mut self, dt: f32) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 100.0) } else { 16.0 };
        self.ui_time += dt;
        self.decay(dt);
        match self.phase {
            Phase::Playing => {
                self.time_ms += dt;
                self.tick_input(dt);
                self.tick_gravity(dt);
                self.tick_lock(dt);
            }
            Phase::Clearing => {
                self.time_ms += dt;
                self.clear_timer += dt;
                if self.clear_timer >= CLEAR_ANIM_MS {
                    self.finish_clear();
                }
            }
            _ => {}
        }
    }

    fn decay(&mut self, dt: f32) {
        if self.msg.timer > 1.0e6 {
            // long-lived banner (game over): keep it, just bleed the counter
            self.msg.timer -= dt;
        } else if self.msg.timer > 0.0 {
            self.msg.timer = (self.msg.timer - dt).max(0.0);
        }
        if self.sub_msg.timer > 0.0 && self.sub_msg.timer < 1.0e6 {
            self.sub_msg.timer = (self.sub_msg.timer - dt).max(0.0);
        }
        if self.flash > 0.0 {
            self.flash = (self.flash - dt).max(0.0);
        }
        if self.shake > 0.0 {
            self.shake = (self.shake - dt * 1.2).max(0.0);
        }
        if self.spawn_anim > 0.0 {
            self.spawn_anim = (self.spawn_anim - dt).max(0.0);
        }
    }

    fn tick_gravity(&mut self, dt: f32) {
        let g = gravity_ms(self.level);
        let soft = self.held[Action::SoftDrop as usize];
        let interval = if soft {
            (g / SOFT_DROP_FACTOR).max(3.0)
        } else {
            g
        };
        self.gravity_acc += dt;
        let mut guard = 0;
        while self.gravity_acc >= interval && guard < 48 {
            guard += 1;
            self.gravity_acc -= interval;
            if !self.step_down(soft) {
                self.gravity_acc = 0.0;
                break;
            }
        }
    }

    fn tick_lock(&mut self, dt: f32) {
        self.grounded = self.board.collides(&self.piece.translated(0, 1));
        if !self.grounded {
            return;
        }
        self.lock_timer += dt;
        if self.lock_timer >= LOCK_DELAY {
            self.lock();
        }
    }

    /// Write the piece into the stack and resolve whatever that causes.
    pub fn lock(&mut self) {
        if self.phase != Phase::Playing {
            return;
        }
        self.events |= ev::LOCK;
        let tspin = self.detect_tspin();
        let all_hidden = self.piece.occupied().iter().all(|&(_, y)| y < HIDDEN as i32);
        if !self.board.lock(&self.piece) {
            self.game_over("TOPPED OUT");
            return;
        }
        self.pieces_placed += 1;
        self.can_hold = true;
        self.das_timer = 0.0;
        self.arr_acc = 0.0;

        if all_hidden {
            self.game_over("TOO MANY BLOCKS");
            return;
        }

        let rows = self.board.full_rows();
        if rows.is_empty() {
            self.combo = -1;
            self.spawn_next();
            return;
        }

        let n = rows.len().min(MAX_CLEAR_ROWS);
        for i in 0..n {
            self.clear_rows[i] = rows[i];
        }
        self.n_clear = n;
        self.clear_timer = 0.0;
        self.phase = Phase::Clearing;
        self.flash = if n == 4 { 220.0 } else { 90.0 };
        self.award(n, tspin);
    }

    /// Once the flash animation ends: drop the rows, update the counters, continue.
    fn finish_clear(&mut self) {
        let n = self.n_clear;
        let rows: Vec<usize> = self.clear_rows[..n].to_vec();
        self.board.remove_rows(&rows);
        self.n_clear = 0;
        self.lines += n as u32;
        if n == 4 {
            self.tetrises += 1;
        }
        let prev = self.level;
        self.level = (1 + self.lines / 10).min(20);
        if self.level > prev {
            self.events |= ev::LEVEL_UP;
            self.flash = self.flash.max(160.0);
        }
        if self.board.is_empty() {
            let bonus = if n == 4 { 3200 } else { 2000 } * self.level;
            self.score += bonus;
            self.events |= ev::PERFECT;
            self.msg_begin("PERFECT CLEAR", 1600.0, true);
            self.sub_msg_begin();
            self.sub_msg.push("+");
            self.sub_msg.push_num(bonus as i64);
            self.sub_msg.timer = 1600.0;
        }
        self.phase = Phase::Playing;
        self.spawn_next();
    }

    /// 0 = none, 1 = mini, 2 = full T-spin, judged from the corner fill.
    fn detect_tspin(&self) -> u32 {
        if self.piece.kind != Kind::T || !self.last_rotate {
            return 0;
        }
        let (cx, cy) = self.piece.centre();
        let filled = |dx: i32, dy: i32| self.board.get(cx + dx, cy + dy) != 0;
        let total = [(-1, -1), (1, -1), (-1, 1), (1, 1)]
            .iter()
            .filter(|(dx, dy)| filled(*dx, *dy))
            .count();
        if total < 3 {
            return 0;
        }
        let front: [(i32, i32); 2] = match self.piece.rot & 3 {
            0 => [(-1, -1), (1, -1)],
            1 => [(1, -1), (1, 1)],
            2 => [(-1, 1), (1, 1)],
            _ => [(-1, -1), (-1, 1)],
        };
        let n_front = front.iter().filter(|(dx, dy)| filled(*dx, *dy)).count();
        if n_front == 2 || self.kick_used >= 2 {
            2
        } else {
            1
        }
    }

    /// Score a lock that cleared `n` rows and raise the matching popups.
    fn award(&mut self, n: usize, tspin: u32) {
        let lvl = self.level;
        let mut pts: u32 = match (tspin, n) {
            (0, 1) => 100,
            (0, 2) => 300,
            (0, 3) => 500,
            (0, 4) => 800,
            (1, 0) => 100,
            (1, 1) => 200,
            (1, 2) => 400,
            (1, 3) => 600,
            (2, 0) => 400,
            (2, 1) => 800,
            (2, 2) => 1200,
            (2, 3) => 1600,
            _ => 0,
        } * lvl;

        let difficult = n == 4 || (tspin > 0 && n > 0);
        let mut b2b_bonus = false;
        if n > 0 {
            if difficult {
                if self.b2b >= 1 {
                    pts += pts / 2;
                    b2b_bonus = true;
                }
                self.b2b += 1;
                if self.b2b > self.best_b2b {
                    self.best_b2b = self.b2b;
                }
            } else {
                self.b2b = 0;
            }
            self.combo += 1;
            if self.combo > self.max_combo {
                self.max_combo = self.combo;
            }
            if self.combo > 0 {
                pts += 50 * self.combo as u32 * lvl;
                self.events |= ev::COMBO;
            }
        }
        self.score += pts;

        if n > 0 {
            self.events |= ev::LINE_CLEAR;
        }
        if n == 4 {
            self.events |= ev::TETRIS;
        }
        if tspin > 0 {
            self.events |= ev::T_SPIN;
            self.tspins += 1;
        }

        self.msg_begin("", 1300.0, n >= 3 || tspin > 0);
        if tspin > 0 {
            self.msg.push(if tspin == 2 { "T-SPIN " } else { "T-SPIN MINI " });
        }
        match n {
            0 => {}
            1 => self.msg.push("SINGLE"),
            2 => self.msg.push("DOUBLE"),
            3 => self.msg.push("TRIPLE"),
            _ => self.msg.push("TETRIS!"),
        }
        self.sub_msg_begin();
        self.sub_msg.push("+");
        self.sub_msg.push_num(pts as i64);
        if self.b2b >= 1 && difficult {
            self.sub_msg.push("  B2B X");
            self.sub_msg.push_num(self.b2b as i64);
        }
        if self.combo > 0 {
            self.sub_msg.push("  COMBO X");
            self.sub_msg.push_num(self.combo as i64);
        }
        if b2b_bonus {
            self.flash = self.flash.max(180.0);
        }
        self.sub_msg.timer = 1300.0;
    }

    // ---- Popup helpers ----------------------------------------------------

    fn msg_begin(&mut self, s: &str, ms: f32, big: bool) {
        self.msg.len = 0;
        self.msg.big = big;
        self.msg.timer = ms;
        self.msg.total = ms;
        self.msg.push(s);
    }

    fn sub_msg_begin(&mut self) {
        self.sub_msg.len = 0;
        self.sub_msg.timer = 0.0;
        self.sub_msg.big = false;
    }

    /// Overwrite the long-lived banner (used for game over).
    pub fn set_msg(&mut self, s: &str, ms: f32, big: bool) {
        self.msg_begin(s, ms, big);
    }
}

impl Msg {
    fn push_byte(&mut self, b: u8) {
        if self.len < self.text.len() {
            self.text[self.len] = b;
            self.len += 1;
        }
    }

    pub fn push(&mut self, s: &str) {
        for b in s.bytes() {
            self.push_byte(b);
        }
    }

    pub fn push_num(&mut self, n: i64) {
        let mut v = n;
        if v < 0 {
            self.push("-");
            v = v.wrapping_neg();
        }
        let mut tmp = [0u8; 21];
        let mut i = 0;
        loop {
            tmp[i] = b'0' + (v % 10) as u8;
            i += 1;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        while i > 0 {
            i -= 1;
            self.push_byte(tmp[i]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Park a copy of a piece exactly where gravity would leave it.
    fn resting(g: &Game, p: Piece) -> Piece {
        let mut p = p;
        while !g.board.collides(&p.translated(0, 1)) {
            p = p.translated(0, 1);
        }
        p
    }

    /// Run the clear animation out (or just let gravity tick) until play resumes.
    fn until_playing(g: &mut Game, frames: usize) {
        for _ in 0..frames {
            if g.phase == Phase::Playing {
                return;
            }
            g.frame(16.0);
        }
        panic!("never returned to the Playing phase");
    }

    #[test]
    fn locking_a_full_row_clears_lines_and_scores() {
        let mut g = Game::new(1);
        g.start();
        g.spawn(Kind::O);

        // Wall off the two rows the O would land in, leaving only its own columns open.
        let p = resting(&g, g.piece);
        let cols: Vec<i32> = p.occupied().iter().map(|c| c.0).collect();
        for (_, row) in p.occupied().iter() {
            for x in 0..COLS as i32 {
                if !cols.contains(&x) {
                    g.board.set(x, *row, 5);
                }
            }
        }

        g.piece = p;
        g.lock();
        assert_eq!(g.phase, Phase::Clearing);
        assert_eq!(g.n_clear, 2, "the O should complete a double");
        assert_ne!(g.events & ev::LINE_CLEAR, 0);

        until_playing(&mut g, 120);
        assert_eq!(g.lines, 2);
        assert!(g.score >= 300, "a double is worth 300 at level 1, got {}", g.score);
        assert_eq!(g.board.visible_blocks(), 0, "the field should be wiped");
        assert_ne!(g.events & ev::PERFECT, 0, "wiping the field is a perfect clear");
    }

    #[test]
    fn clearing_a_row_drops_the_stack_above_it() {
        let mut g = Game::new(3);
        g.start();
        let bottom = TOTAL_ROWS - 1;
        for x in 2..COLS {
            g.board.set(x as i32, bottom as i32, 1); // every column but the first two
        }
        g.board.set(5, bottom as i32 - 3, 2); // a marker that must fall by exactly one row

        g.spawn(Kind::O);
        let mut p = g.piece;
        p.x = 0; // slide into the gap and drop it flush with the floor
        p.y = bottom as i32 - 1;
        g.piece = p;
        g.lock();

        until_playing(&mut g, 240);
        assert_eq!(g.lines, 1);
        assert_eq!(g.board.get(5, (bottom - 3) as i32), 0);
        assert_eq!(
            g.board.get(5, (bottom - 2) as i32),
            2,
            "blocks above a cleared row fall by exactly one"
        );
    }

    #[test]
    fn hold_swaps_once_per_piece() {
        let mut g = Game::new(7);
        g.start();
        let first = g.piece.kind;
        g.hold();
        assert!(g.has_hold);
        assert_eq!(g.hold, first);
        assert!(!g.can_hold, "hold must lock until the next piece");
        let current = g.piece.kind;
        g.hold();
        assert_eq!(g.piece.kind, current, "a second hold in one piece is refused");
        assert_eq!(g.holds_used, 1);
    }

    #[test]
    fn hard_drop_lands_scores_and_hands_over() {
        let mut g = Game::new(11);
        g.start();
        let dist = g.hard_drop();
        assert!(dist > 0);
        assert_eq!(g.pieces_placed, 1);
        assert!(g.score >= (dist * 2) as u32, "2 points per cell dropped");
        assert_ne!(g.events & ev::HARD_DROP, 0);
        assert_eq!(g.phase, Phase::Playing, "the next piece should already be live");
    }

    #[test]
    fn a_blocked_spawn_ends_the_game() {
        let mut g = Game::new(13);
        g.start();
        for y in (HIDDEN - 1)..TOTAL_ROWS {
            for x in 0..COLS {
                g.board.set(x as i32, y as i32, 3);
            }
        }
        g.spawn(Kind::I);
        assert_eq!(g.phase, Phase::GameOver);
        assert_ne!(g.events & ev::GAME_OVER, 0);
        assert!(g.best >= g.score, "the best score is banked on game over");
    }

    #[test]
    fn das_repeats_a_held_direction() {
        let mut g = Game::new(17);
        g.start();
        g.spawn(Kind::O);
        let x0 = g.piece.x;
        g.key_down(Action::Left);
        for _ in 0..30 {
            g.frame(16.0); // ~480ms: well past DAS
        }
        assert!(g.piece.x < x0, "a held key should keep shifting the piece");
        assert!(g.piece.x >= 0, "and never through a wall");
        g.key_up(Action::Left);
    }

    #[test]
    fn level_gravity_speeds_up() {
        assert!(gravity_ms(1) > gravity_ms(5));
        assert!(gravity_ms(5) > gravity_ms(10));
        assert!(gravity_ms(20) >= 16.0);
    }

    #[test]
    fn the_dealer_uses_a_seven_bag() {
        let mut g = Game::new(21);
        g.start();
        let mut seen = [0u8; 7];
        seen[g.piece.kind as usize] += 1;
        for i in 0..6 {
            let dir = if i % 2 == 0 { -1 } else { 1 };
            for _ in 0..6 {
                g.try_move(dir); // spread the stack out so nothing tops out
            }
            g.hard_drop();
            until_playing(&mut g, 240);
            assert_eq!(g.phase, Phase::Playing);
            seen[g.piece.kind as usize] += 1;
        }
        assert!(
            seen.iter().all(|&n| n == 1),
            "seven pieces should be one of each: {seen:?}"
        );
    }

    /// Cell layout of the stack plus a candidate piece, so a placement can be scored.
    fn layout(b: &Board, p: &Piece) -> [[bool; COLS]; TOTAL_ROWS] {
        let mut m = [[false; COLS]; TOTAL_ROWS];
        for y in 0..TOTAL_ROWS {
            for x in 0..COLS {
                m[y][x] = b.get(x as i32, y as i32) != 0;
            }
        }
        for (x, y) in p.occupied() {
            if x >= 0 && (x as usize) < COLS && y >= 0 && (y as usize) < TOTAL_ROWS {
                m[y as usize][x as usize] = true;
            }
        }
        m
    }

    /// Rough stack quality: low, flat and hole-free beats tall and ragged.
    fn placement_cost(b: &Board, p: &Piece) -> i32 {
        let m = layout(b, p);
        let mut agg = 0;
        let mut holes = 0;
        let mut bump = 0;
        let mut peak = 0;
        let mut prev = 0;
        let mut cleared = 0;
        for x in 0..COLS {
            let top = (0..TOTAL_ROWS).find(|&y| m[y][x]).unwrap_or(TOTAL_ROWS);
            let h = (TOTAL_ROWS - top) as i32;
            agg += h;
            peak = peak.max(h);
            if x > 0 {
                bump += (h - prev).abs();
            }
            prev = h;
            holes += (top + 1..TOTAL_ROWS).filter(|&y| !m[y][x]).count();
        }
        for y in HIDDEN..TOTAL_ROWS {
            if m[y].iter().all(|&c| c) {
                cleared += 1;
            }
        }
        agg * 2 + holes as i32 * 15 + bump * 4 + peak * 8 - cleared * 60
    }

    #[test]
    fn a_greedy_bot_keeps_the_well_alive() {
        // Play a full game by picking the cheapest column for every piece. This is the
        // closest thing to an end-to-end regression test: gravity, locks, line clears,
        // level ups and the spawn/top-out rules all get hammered at once.
        let mut g = Game::new(20261004);
        g.start();
        let mut placed = 0;
        for _ in 0..400 {
            let mut guard = 0;
            while g.phase != Phase::Playing && g.phase != Phase::GameOver && guard < 400 {
                g.frame(16.7);
                guard += 1;
            }
            if g.phase == Phase::GameOver {
                break;
            }
            let base = g.piece;
            let mut best = i32::MAX;
            let mut pick = (base.rot, base.x);
            for rot in 0..4u32 {
                let turned = base.rotated(rot);
                let cells = turned.occupied();
                let left = cells.iter().map(|c| c.0).min().unwrap();
                let right = cells.iter().map(|c| c.0).max().unwrap();
                for shift in -left..=(COLS as i32 - 1 - right) {
                    let mut cand = turned.translated(shift, 0);
                    if g.board.collides(&cand) {
                        continue;
                    }
                    cand = resting(&g, cand);
                    let cost = placement_cost(&g.board, &cand);
                    if cost < best {
                        best = cost;
                        pick = (cand.rot, cand.x);
                    }
                }
            }
            g.piece.rot = pick.0;
            g.piece.x = pick.1;
            g.hard_drop();
            placed += 1;
        }
        let mut grid = String::new();
        for y in 0..TOTAL_ROWS {
            for x in 0..COLS {
                grid.push(if g.board.get(x as i32, y as i32) == 0 { '.' } else { '#' });
            }
            grid.push('\n');
        }
        assert!(placed >= 120, "bot only placed {placed} pieces\n{grid}");
        assert_ne!(g.phase, Phase::GameOver, "bot topped out after {placed}\n{grid}");
        assert!(g.lines >= 20, "bot only cleared {} lines\n{grid}", g.lines);
        assert!(g.level >= 2, "level never rose past {}\n{grid}", g.level);
        assert!(
            g.pieces_placed >= 120,
            "counter disagrees with the bot: {} vs {placed}",
            g.pieces_placed
        );
        assert_ne!(g.events & ev::LINE_CLEAR, 0, "no clear event was ever raised");
    }
}


