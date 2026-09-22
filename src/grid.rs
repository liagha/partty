use std::collections::VecDeque;
use vte::{Params, Perform};

use crate::color::{Color, Palette};

const PAST: usize = 1000;

#[derive(Clone, Copy, PartialEq)]
struct Cell {
    glyph: char,
    fg: Color,
    bg: Color,
    under: bool,
    strike: bool,
    bold: bool,
}

struct Pen {
    fg: Color,
    bg: Color,
    bold: bool,
    slot: Option<u8>,
    rev: bool,
    under: bool,
    strike: bool,
}

impl Pen {
    fn of(inks: &Palette) -> Self {
        Self {
            fg: inks.fore(),
            bg: inks.back(),
            bold: false,
            slot: None,
            rev: false,
            under: false,
            strike: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Default)]
enum Shape {
    #[default]
    Block,
    Under,
    Bar,
}

impl Shape {
    fn mark(self) -> char {
        match self {
            Shape::Block => '█',
            Shape::Under => '▁',
            Shape::Bar => '▏',
        }
    }
}

pub struct Span {
    pub hue: Color,
    pub back: Option<Color>,
    pub text: String,
    pub under: bool,
    pub strike: bool,
    pub bold: bool,
    pub col: usize,
}

pub struct Grid {
    cells: Vec<Vec<Cell>>,
    alt: Vec<Vec<Cell>>,
    live: bool,
    save: (usize, usize),
    show: bool,
    past: VecDeque<Vec<Cell>>,
    off: i32,
    carry: f32,
    below: usize,
    dirty: bool,
    pen: Pen,
    inks: Palette,
    rows: usize,
    cols: usize,
    top: usize,
    bot: usize,
    row: usize,
    col: usize,
    reply: Vec<u8>,
    mark: Option<((usize, usize), (usize, usize))>,
    press: bool,
    cell: bool,
    ext: bool,
    paste: bool,
    focus: bool,
    shape: Shape,
    blink: bool,
    phase: bool,
    title: Option<String>,
}

impl Grid {
    pub fn new(rows: usize, cols: usize, below: usize, inks: Palette) -> Self {
        let mut grid = Self {
            cells: vec![],
            alt: vec![],
            live: false,
            save: (0, 0),
            show: true,
            past: VecDeque::new(),
            off: 0,
            top: 0,
            bot: 0,
            carry: 0.0,
            below,
            dirty: false,
            pen: Pen::of(&inks),
            inks,
            rows: 0,
            cols: 0,
            row: 0,
            col: 0,
            reply: Vec::new(),
            mark: None,
            press: false,
            cell: false,
            ext: false,
            paste: false,
            focus: false,
            shape: Shape::default(),
            blink: false,
            phase: true,
            title: None,
        };
        grid.resize(rows, cols);
        grid
    }

    pub fn blinked(&self) -> bool {
        self.blink
    }

    pub fn flip(&mut self) {
        self.phase = !self.phase;
        self.dirty = true;
    }

    pub fn lit(&mut self) {
        if !self.phase {
            self.phase = true;
            self.dirty = true;
        }
    }

    pub fn take_reply(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.reply)
    }

    pub fn mouse(&self) -> bool {
        self.press
    }

    pub fn motion(&self) -> bool {
        self.cell
    }

    pub fn click(&mut self, btn: u8, row: usize, col: usize, down: bool) {
        if down {
            self.report(btn, row, col, true);
        } else {
            self.report(3, row, col, false);
        }
    }

    pub fn roll(&mut self, row: usize, col: usize, up: bool) {
        self.report(if up { 64 } else { 65 }, row, col, true);
    }

    fn report(&mut self, code: u8, row: usize, col: usize, down: bool) {
        let x = col + 1;
        let y = row + 1;
        if self.ext {
            self.answer(&format!(
                "\x1b[<{code};{x};{y}{}",
                if down { 'M' } else { 'm' }
            ));
        } else {
            self.reply.extend_from_slice(&[
                0x1b,
                b'[',
                b'M',
                (32 + code as usize).min(255) as u8,
                (32 + x).min(255) as u8,
                (32 + y).min(255) as u8,
            ]);
        }
    }

    pub fn take_title(&mut self) -> Option<String> {
        self.title.take()
    }

    pub fn pastes(&self) -> bool {
        self.paste
    }

    pub fn focused(&self) -> bool {
        self.focus
    }

    pub fn focus(&mut self, inside: bool) {
        self.answer(if inside { "\x1b[I" } else { "\x1b[O" });
    }

    fn answer(&mut self, text: &str) {
        self.reply.extend_from_slice(text.as_bytes());
    }

    fn empty(&self) -> Cell {
        Cell {
            glyph: ' ',
            fg: self.pen.fg,
            bg: self.pen.bg,
            under: self.pen.under,
            strike: self.pen.strike,
            bold: self.pen.bold,
        }
    }

    pub fn resize(&mut self, rows: usize, cols: usize) {
        let fill = self.empty();
        self.cells = vec![vec![fill; cols]; rows];
        self.alt = vec![vec![fill; cols]; rows];
        self.rows = rows;
        self.cols = cols;
        self.row = 0;
        self.col = 0;
        self.top = 0;
        self.bot = rows.saturating_sub(1);
        self.dirty = true;
        self.mark = None;
        self.off = self
            .off
            .clamp(-(self.below as i32), self.past.len() as i32);
    }

    pub fn cursor(&self) -> (usize, usize, bool, bool) {
        (self.row, self.col, self.live, self.show)
    }

    pub fn moved(&mut self, was: (usize, usize, bool, bool)) {
        self.dirty |= self.cursor() != was;
    }

    fn enter(&mut self, clear: bool) {
        if self.live {
            return;
        }
        self.save = (self.row, self.col);
        std::mem::swap(&mut self.cells, &mut self.alt);
        self.row = 0;
        self.col = 0;
        self.off = 0;
        self.top = 0;
        self.bot = self.rows.saturating_sub(1);
        if clear {
            let fill = self.empty();
            for row in &mut self.cells {
                row.fill(fill);
            }
        }
        self.live = true;
        self.dirty = true;
        self.mark = None;
    }

    fn exit(&mut self) {
        if !self.live {
            return;
        }
        std::mem::swap(&mut self.cells, &mut self.alt);
        (self.row, self.col) = self.save;
        self.live = false;
        self.top = 0;
        self.bot = self.rows.saturating_sub(1);
        self.off = 0;
        self.dirty = true;
        self.mark = None;
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::replace(&mut self.dirty, false)
    }

    pub fn window(&self) -> (usize, usize) {
        let base = if self.live { 0 } else { self.past.len() };
        let total = base + self.rows;
        let end = total as i32 - self.off;
        let stop = end.clamp(0, total as i32) as usize;
        let start = (end - self.rows as i32).max(0).min(stop as i32) as usize;
        (start, stop)
    }

    fn line(&self, i: usize, base: usize) -> &Vec<Cell> {
        if !self.live && i < self.past.len() {
            &self.past[i]
        } else {
            &self.cells[i - base]
        }
    }

    pub fn at(&self, vrow: usize, vcol: usize) -> Option<(usize, usize)> {
        if self.cols == 0 {
            return None;
        }
        let (start, stop) = self.window();
        (vrow < stop.saturating_sub(start))
            .then(|| (start + vrow, vcol.min(self.cols - 1)))
    }

    pub fn select(&mut self, from: (usize, usize), to: (usize, usize)) {
        let mark = Some(if from <= to { (from, to) } else { (to, from) });
        if self.mark != mark {
            self.mark = mark;
            self.dirty = true;
        }
    }

    pub fn unmark(&mut self) {
        if self.mark.take().is_some() {
            self.dirty = true;
        }
    }

    pub fn marked(&self) -> bool {
        self.mark.is_some()
    }

    fn inside(&self, row: usize, col: usize) -> bool {
        match self.mark {
            Some((from, to)) => (row, col) >= from && (row, col) <= to,
            None => false,
        }
    }

    pub fn selected(&self) -> String {
        let Some(((r1, c1), (r2, c2))) = self.mark else {
            return String::new();
        };
        let base = if self.live { 0 } else { self.past.len() };
        let total = base + self.rows;
        let mut lines = Vec::new();
        for i in r1..=r2.min(total.saturating_sub(1)) {
            if i >= total {
                break;
            }
            let row = self.line(i, base);
            let from = if i == r1 { c1.min(row.len()) } else { 0 };
            let to = if i == r2 {
                (c2 + 1).min(row.len())
            } else {
                row.len()
            };
            let mut text: String = row[from..to].iter().map(|cell| cell.glyph).collect();
            while text.ends_with(' ') {
                text.pop();
            }
            lines.push(text);
        }
        lines.join("\n")
    }

    fn wipe(&mut self, row: usize, from: usize, to: usize) {
        let to = to.min(self.cols);
        let fill = self.empty();
        if self.cells[row][from..to].iter().any(|&c| c != fill) {
            self.cells[row][from..to].fill(fill);
            self.dirty = true;
        }
    }

    pub fn wheel(&mut self, lines: f32) {
        if self.live {
            self.carry = 0.0;
            return;
        }
        self.carry += lines;
        let whole = self.carry.trunc() as i32;
        self.carry -= whole as f32;
        let off = self.off + whole;
        let clamped = off.clamp(-(self.below as i32), self.past.len() as i32);
        self.dirty = clamped != self.off;
        self.off = clamped;
    }

    fn scroll(&mut self) {
        if self.live {
            self.cells.remove(0);
            let fill = self.empty();
            self.cells.push(vec![fill; self.cols]);
            self.row = self.rows - 1;
            self.mark = None;
            return;
        }
        if self.top > 0 || self.bot + 1 < self.rows {
            self.lift(1);
            self.row = self.bot;
            return;
        }
        let top = self.cells.remove(0);
        self.past.push_back(top);
        if self.past.len() > PAST {
            self.past.pop_front();
            if let Some(((r1, c1), (r2, c2))) = self.mark {
                self.mark = Some((
                    (r1.saturating_sub(1), c1),
                    (r2.saturating_sub(1), c2),
                ));
            }
        }
        if self.off > 0 {
            self.off = (self.off + 1).min(self.past.len() as i32);
        } else if self.off < 0 {
            self.off = 0;
        }
        let fill = self.empty();
        self.cells.push(vec![fill; self.cols]);
        self.row = self.rows - 1;
    }

    fn lift(&mut self, n: usize) {
        let fill = self.empty();
        for _ in 0..n.min(self.bot - self.top + 1) {
            self.cells.remove(self.top);
            self.cells.insert(self.bot, vec![fill; self.cols]);
        }
        self.dirty = true;
        self.mark = None;
    }

    fn drop(&mut self, n: usize) {
        let fill = self.empty();
        for _ in 0..n.min(self.bot - self.top + 1) {
            self.cells.remove(self.bot);
            self.cells.insert(self.top, vec![fill; self.cols]);
        }
        self.dirty = true;
        self.mark = None;
    }

    fn down(&mut self) -> bool {
        if self.row + 1 > self.bot {
            self.scroll();
            true
        } else {
            self.row += 1;
            false
        }
    }

    #[cfg(test)]
    pub fn text(&self) -> String {
        self.spans()
            .iter()
            .map(|line| {
                line.iter()
                    .map(|span| span.text.as_str())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn spans(&self) -> Vec<Vec<Span>> {
        let base = if self.live { 0 } else { self.past.len() };
        let (start, stop) = self.window();
        let at = if self.show && self.off == 0 {
            Some((base + self.row, self.col.min(self.cols.saturating_sub(1))))
        } else {
            None
        };
        let mut out: Vec<Vec<Span>> = (start..stop)
            .map(|i| {
                self.runs(
                    i,
                    self.line(i, base),
                    at.filter(|&(r, _)| r == i).map(|(_, c)| c),
                )
            })
            .collect();
        while out.last().is_some_and(|line| line.is_empty()) {
            out.pop();
        }
        out.extend(
            std::iter::repeat_with(Vec::new)
                .take((-self.off).max(0).min(self.rows as i32) as usize),
        );
        out
    }

    fn runs(&self, flat: usize, row: &Vec<Cell>, cur: Option<usize>) -> Vec<Span> {
        let mut end = row.len();
        while end > 0 {
            let cell = &row[end - 1];
            if cell.glyph != ' '
                || cell.bg != self.inks.back()
                || cell.under
                || cell.strike
                || self.inside(flat, end - 1)
            {
                break;
            }
            end -= 1;
        }
        if let Some(c) = cur {
            end = end.max((c + 1).min(row.len()));
        }
        let mut spans: Vec<Span> = Vec::new();
        for (i, cell) in row[..end].iter().enumerate() {
            let glyph = if cur == Some(i) && (!self.blink || self.phase) {
                self.shape.mark()
            } else {
                cell.glyph
            };
            let flip = self.inside(flat, i);
            let hue = if flip { cell.bg } else { cell.fg };
            let plain = cell.bg == self.inks.back();
            let back = if flip {
                if cell.fg == self.inks.back() {
                    None
                } else {
                    Some(cell.fg)
                }
            } else if plain {
                None
            } else {
                Some(cell.bg)
            };
            match spans.last_mut() {
                Some(span)
                    if span.hue == hue
                        && span.back == back
                        && span.under == cell.under
                        && span.strike == cell.strike
                        && span.bold == cell.bold =>
                {
                    span.text.push(glyph)
                }
                _ => spans.push(Span {
                    hue,
                    back,
                    text: glyph.to_string(),
                    under: cell.under,
                    strike: cell.strike,
                    bold: cell.bold,
                    col: i,
                }),
            }
        }
        spans
    }
}

impl Default for Grid {
    fn default() -> Self {
        Self::new(24, 80, 24, Palette::default())
    }
}

impl Perform for Grid {
    fn print(&mut self, c: char) {
        let mut moved = false;
        if self.col >= self.cols {
            moved = self.down();
            self.col = 0;
        }
        let (fg, bg) = if self.pen.rev {
            (self.pen.bg, self.pen.fg)
        } else {
            (self.pen.fg, self.pen.bg)
        };
        let fill = Cell {
            glyph: c,
            fg,
            bg,
            under: self.pen.under,
            strike: self.pen.strike,
            bold: self.pen.bold,
        };
        if moved || self.cells[self.row][self.col] != fill {
            self.cells[self.row][self.col] = fill;
            self.dirty = true;
        }
        self.col += 1;
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' | b'\x0b' | b'\x0c' => self.dirty |= self.down(),
            b'\r' => self.col = 0,
            b'\x07' => {}
            b'\x08' => self.col = self.col.saturating_sub(1),
            b'\t' => self.col = ((self.col / 8) + 1) * 8 .min(self.cols.saturating_sub(1)),
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, mid: &[u8], _ignore: bool, action: char) {
        let arg = |i: usize, default: u16| {
            params
                .iter()
                .nth(i)
                .and_then(|group| group.first().copied())
                .filter(|&v| v != 0)
                .unwrap_or(default)
        };
        let maxr = self.rows - 1;
        let maxc = self.cols - 1;
        let edge = self.col.min(maxc);
        match action {
            'A' => self.row = self.row.saturating_sub(arg(0, 1) as usize).max(self.top),
            'B' => self.row = (self.row + arg(0, 1) as usize).min(self.bot),
            'C' => self.col = (self.col + arg(0, 1) as usize).min(maxc),
            'D' => self.col = self.col.saturating_sub(arg(0, 1) as usize),
            'E' => {
                self.row = (self.row + arg(0, 1) as usize).min(self.bot);
                self.col = 0;
            }
            'F' => {
                self.row = self.row.saturating_sub(arg(0, 1) as usize).max(self.top);
                self.col = 0;
            }
            'G' => self.col = (arg(0, 1) as usize).saturating_sub(1).min(maxc),
            'H' | 'f' => {
                self.row = (arg(0, 1) as usize)
                    .saturating_sub(1)
                    .clamp(self.top, self.bot);
                self.col = (arg(1, 1) as usize).saturating_sub(1).min(maxc);
            }
            'd' => {
                self.row = (arg(0, 1) as usize)
                    .saturating_sub(1)
                    .clamp(self.top, self.bot)
            }
            'J' => match arg(0, 0) {
                1 => {
                    for r in 0..self.row {
                        self.wipe(r, 0, self.cols);
                    }
                    self.wipe(self.row, 0, edge + 1);
                }
                2 | 3 => {
                    for r in 0..self.rows {
                        self.wipe(r, 0, self.cols);
                    }
                }
                _ => {
                    self.wipe(self.row, edge, self.cols);
                    for r in self.row + 1..self.rows {
                        self.wipe(r, 0, self.cols);
                    }
                }
            },
            'K' => match arg(0, 0) {
                1 => self.wipe(self.row, 0, edge + 1),
                2 => self.wipe(self.row, 0, self.cols),
                _ => self.wipe(self.row, edge, self.cols),
            },
            'X' => self.wipe(self.row, edge, edge + arg(0, 1) as usize),
            'q' => {
                if mid == [b' '] {
                    let n = arg(0, 0);
                    self.shape = match n {
                        3 | 4 => Shape::Under,
                        5 | 6 => Shape::Bar,
                        _ => Shape::Block,
                    };
                    self.blink = matches!(n, 0 | 1 | 3 | 5);
                    self.phase = true;
                    self.dirty = true;
                }
            }
            'r' => {
                let top = (arg(0, 1) as usize).saturating_sub(1);
                let bot = (arg(1, self.rows as u16) as usize).saturating_sub(1);
                if top < bot && bot < self.rows {
                    self.top = top;
                    self.bot = bot;
                    self.row = top;
                    self.col = 0;
                }
            }
            'S' => self.lift(arg(0, 1) as usize),
            'T' => self.drop(arg(0, 1) as usize),
            'L' => {
                let row = self.row.clamp(self.top, self.bot);
                let n = (arg(0, 1) as usize).min(self.bot - row + 1);
                let fill = self.empty();
                for _ in 0..n {
                    self.cells.remove(self.bot);
                    self.cells.insert(row, vec![fill; self.cols]);
                }
                self.dirty = true;
                self.mark = None;
            }
            'M' => {
                let row = self.row.clamp(self.top, self.bot);
                let n = (arg(0, 1) as usize).min(self.bot - row + 1);
                let fill = self.empty();
                for _ in 0..n {
                    self.cells.remove(row);
                    self.cells.insert(self.bot, vec![fill; self.cols]);
                }
                self.dirty = true;
                self.mark = None;
            }
            'm' => {
                if mid.is_empty() {
                    self.sgr(params);
                }
            }
            'c' => {
                if mid.contains(&b'>') {
                    self.answer("\x1b[>0;10;0c");
                } else {
                    self.answer("\x1b[?6c");
                }
            }
            'n' => match arg(0, 0) {
                5 => self.answer("\x1b[0n"),
                6 => {
                    let row = self.row.min(maxr) + 1;
                    let col = self.col.min(maxc) + 1;
                    self.answer(&format!("\x1b[{row};{col}R"));
                }
                _ => {}
            },
            't' => {
                if arg(0, 0) == 18 {
                    self.answer(&format!("\x1b[8;{};{}t", self.rows, self.cols));
                }
            }
            'h' | 'l' => {
                let set = action == 'h';
                if mid.contains(&b'?') {
                    for group in params.iter() {
                        for &p in group.iter() {
                            match p {
                                25 => {
                                    self.show = set;
                                    self.dirty = true;
                                }
                                1000 => {
                                    self.press = set;
                                }
                                1002 => {
                                    self.cell = set;
                                }
                                1006 => {
                                    self.ext = set;
                                }
                                2004 => {
                                    self.paste = set;
                                }
                                1004 => {
                                    self.focus = set;
                                }
                                1047 => {
                                    if set {
                                        self.enter(false);
                                    } else {
                                        self.exit();
                                    }
                                }
                                1049 => {
                                    if set {
                                        self.enter(true);
                                    } else {
                                        self.exit();
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell: bool) {
        if params.len() >= 2 && (params[0] == b"0" || params[0] == b"2") {
            self.title = Some(String::from_utf8_lossy(params[1]).into_owned());
        }
    }
}

impl Grid {
    fn ink(&mut self, slot: u8) {
        let slot = if self.pen.bold && slot < 8 {
            slot + 8
        } else {
            slot
        };
        self.pen.fg = self.inks.dye(slot);
        self.pen.slot = Some(slot);
    }

    fn paper(&mut self, slot: u8) {
        self.pen.bg = self.inks.dye(slot);
    }

    fn sgr(&mut self, params: &Params) {
        let mut flat: Vec<u16> = Vec::new();
        for group in params.iter() {
            flat.extend_from_slice(group);
        }
        if flat.is_empty() {
            flat.push(0);
        }
        let mut i = 0;
        while i < flat.len() {
            match flat[i] {
                0 => self.pen = Pen::of(&self.inks),
                1 => {
                    self.pen.bold = true;
                    if let Some(slot) = self.pen.slot {
                        if slot < 8 {
                            self.pen.slot = Some(slot + 8);
                            self.pen.fg = self.inks.dye(slot + 8);
                        }
                    }
                }
                22 => {
                    self.pen.bold = false;
                    if let Some(slot) = self.pen.slot {
                        if slot >= 8 {
                            self.pen.slot = Some(slot - 8);
                            self.pen.fg = self.inks.dye(slot - 8);
                        }
                    }
                }
                7 => self.pen.rev = true,
                27 => self.pen.rev = false,
                4 => self.pen.under = true,
                24 => self.pen.under = false,
                9 => self.pen.strike = true,
                29 => self.pen.strike = false,
                30..=37 => self.ink((flat[i] - 30) as u8),
                39 => {
                    self.pen.fg = self.inks.fore();
                    self.pen.slot = None;
                }
                40..=47 => self.paper((flat[i] - 40) as u8),
                49 => self.pen.bg = self.inks.back(),
                90..=97 => self.ink((flat[i] - 90 + 8) as u8),
                100..=107 => self.paper((flat[i] - 100 + 8) as u8),
                38 | 48 => {
                    let fg = flat[i] == 38;
                    if flat.get(i + 1) == Some(&5) {
                        if let Some(&idx) = flat.get(i + 2) {
                            let dye = self.inks.dye(idx.min(255) as u8);
                            if fg {
                                self.pen.fg = dye;
                                self.pen.slot = None;
                            } else {
                                self.pen.bg = dye;
                            }
                        }
                        i += 2;
                    } else if flat.get(i + 1) == Some(&2) {
                        if let (Some(&r), Some(&g), Some(&b)) =
                            (flat.get(i + 2), flat.get(i + 3), flat.get(i + 4))
                        {
                            let dye =
                                Color::rgb(r.min(255) as u8, g.min(255) as u8, b.min(255) as u8);
                            if fg {
                                self.pen.fg = dye;
                                self.pen.slot = None;
                            } else {
                                self.pen.bg = dye;
                            }
                        }
                        i += 4;
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn fed(chunks: &[&str]) -> String {
        let mut grid = Grid::new(24, 80, 24, Palette::default());
        grid.show = false;
        let mut parse = vte::Parser::new();
        for chunk in chunks {
            parse.advance(&mut grid, chunk.as_bytes());
        }
        grid.text()
    }

    fn tinted(chunks: &[&str]) -> Vec<Vec<(String, (u8, u8, u8))>> {        let mut grid = Grid::new(24, 80, 24, Palette::default());
        grid.show = false;
        let mut parse = vte::Parser::new();
        for chunk in chunks {
            parse.advance(&mut grid, chunk.as_bytes());
        }
        grid.spans()
            .into_iter()
            .map(|line| {
                line.into_iter()
                    .map(|span| {
                        (
                            span.text,
                            (span.hue.r(), span.hue.g(), span.hue.b()),
                        )
                    })
                    .collect()
            })
            .collect()
    }

    fn rgb(slot: usize) -> (u8, u8, u8) {
        let rgb = Palette::default().slots[slot];
        (rgb[0], rgb[1], rgb[2])
    }

    fn asked(chunks: &[&str]) -> Vec<u8> {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        let mut parse = vte::Parser::new();
        for chunk in chunks {
            parse.advance(&mut grid, chunk.as_bytes());
        }
        grid.take_reply()
    }

    fn marked_grid() -> Grid {
        let mut grid = Grid::new(4, 20, 4, Palette::default());
        grid.show = false;
        grid
    }

    fn six() -> Grid {
        let mut grid = Grid::new(6, 10, 6, Palette::default());
        grid.show = false;
        grid
    }

    #[test]
    fn region_scrolls_inside() {
        let mut grid = six();
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"A\r\nB\r\nC\r\nD\r\nE\r\nF");
        parse.advance(&mut grid, b"\x1b[2;5r\x1b[5;1H\n");
        assert_eq!(grid.text(), "A\nC\nD\nE\n\nF");
    }

    #[test]
    fn su_pulls_up() {
        let mut grid = six();
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"A\r\nB\r\nC\r\nD\r\nE\r\nF");
        parse.advance(&mut grid, b"\x1b[2;5r\x1b[S");
        assert_eq!(grid.text(), "A\nC\nD\nE\n\nF");
    }

    #[test]
    fn sd_pushes_down() {
        let mut grid = six();
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"A\r\nB\r\nC\r\nD\r\nE\r\nF");
        parse.advance(&mut grid, b"\x1b[2;5r\x1b[T");
        assert_eq!(grid.text(), "A\n\nB\nC\nD\nF");
    }

    #[test]
    fn dl_il_shift() {
        let mut grid = six();
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"A\r\nB\r\nC\r\nD\r\nE\r\nF");
        parse.advance(&mut grid, b"\x1b[3;1H\x1b[2L");
        assert_eq!(grid.text(), "A\nB\n\n\nC\nD");
        parse.advance(&mut grid, b"\x1b[2M");
        assert_eq!(grid.text(), "A\nB\nC\nD");
    }

    #[test]
    fn reverse_flips() {
        let mut grid = six();
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"\x1b[7mAB\x1b[27mCD");
        let rows = grid.spans();
        let fore = Palette::default().fore();
        let back = Palette::default().back();
        assert_eq!(rows[0][0].text, "AB");
        assert_eq!(rows[0][0].hue, back);
        assert_eq!(rows[0][0].back, Some(fore));
        assert_eq!(rows[0][1].text, "CD");
        assert_eq!(rows[0][1].hue, fore);
        assert_eq!(rows[0][1].back, None);
    }

    #[test]
    fn cup_clamps_to_region() {
        let mut grid = six();
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"\x1b[2;5r\x1b[1;1H\x1b[6n");
        assert_eq!(grid.take_reply(), b"\x1b[2;1R");
    }

    #[test]
    fn select_reads_text() {
        let mut grid = marked_grid();
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"hello world\r\nsecond\r\n");
        assert_eq!(grid.selected(), "");
        grid.select((0, 0), (0, 4));
        assert_eq!(grid.selected(), "hello");
        grid.select((0, 0), (1, 5));
        assert_eq!(grid.selected(), "hello world\nsecond");
    }

    #[test]
    fn select_flips_video() {
        let mut grid = marked_grid();
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"ab");
        grid.select((0, 0), (0, 0));
        let line = &grid.spans()[0];
        assert_eq!(line.len(), 2);
        assert_ne!(line[0].hue, line[1].hue);
        assert!(line[0].back.is_some());
        assert!(line[1].back.is_none());
    }

    #[test]
    fn scroll_keeps_selection() {
        let mut grid = marked_grid();
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"aa\r\n");
        grid.select((0, 0), (0, 1));
        parse.advance(&mut grid, b"bb\r\ncc\r\ndd\r\n");
        assert_eq!(grid.selected(), "aa");
    }

    #[test]
    fn at_maps_window() {
        let mut grid = marked_grid();
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n");
        let (start, stop) = grid.window();
        assert_eq!(grid.at(0, 0), Some((start, 0)));
        assert_eq!(grid.at(stop - start, 0), None);
    }

    #[test]
    fn dsr_answers_position() {
        assert_eq!(asked(&["\x1b[3;3H", "\x1b[6n"]), b"\x1b[3;3R".to_vec());
    }

    #[test]
    fn dsr_answers_ok() {
        assert_eq!(asked(&["\x1b[5n"]), b"\x1b[0n".to_vec());
    }

    #[test]
    fn da_answers() {
        assert_eq!(asked(&["\x1b[c"]), b"\x1b[?6c".to_vec());
        assert_eq!(asked(&["\x1b[>c"]), b"\x1b[>0;10;0c".to_vec());
    }

    #[test]
    fn winops_answers_size() {
        assert_eq!(asked(&["\x1b[18t"]), b"\x1b[8;4;10t".to_vec());
    }

    #[test]
    fn clicks_report_sgr() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"\x1b[?1000h\x1b[?1006h");
        assert!(grid.mouse());
        grid.click(0, 2, 3, true);
        assert_eq!(grid.take_reply(), b"\x1b[<0;4;3M".to_vec());
        grid.click(0, 2, 3, false);
        assert_eq!(grid.take_reply(), b"\x1b[<3;4;3m".to_vec());
        grid.roll(2, 3, true);
        assert_eq!(grid.take_reply(), b"\x1b[<64;4;3M".to_vec());
        grid.roll(2, 3, false);
        assert_eq!(grid.take_reply(), b"\x1b[<65;4;3M".to_vec());
        parse.advance(&mut grid, b"\x1b[?1000l");
        assert!(!grid.mouse());
    }

    #[test]
    fn clicks_fall_back_to_x10() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"\x1b[?1000h");
        grid.click(1, 0, 0, true);
        assert_eq!(grid.take_reply(), vec![0x1b, b'[', b'M', 33, 33, 33]);
    }

    #[test]
    fn prompt_redraws() {
        assert_eq!(fed(&["%   \r \r", "\r[i] alee @ ~ "]), "[i] alee @ ~");
    }

    #[test]
    fn enter_keeps_single_prompt() {
        let out = fed(&[
            "\rprompt> ",
            "\x1b[?2004l\r\r\n",
            "%   \r \r",
            "\rprompt> ",
        ]);
        assert_eq!(out, "prompt>\nprompt>");
    }

    #[test]
    fn cursor_addresses() {
        assert_eq!(fed(&["\x1b[2;5Hhi"]), "\n    hi");
    }

    #[test]
    fn erase_line() {
        assert_eq!(fed(&["hello\x1b[2K"]), "");
    }

    #[test]
    fn backspace_erases() {
        assert_eq!(fed(&["kk\x08 \x08"]), "k");
    }

    #[test]
    fn split_escape() {
        assert_eq!(fed(&["\x1b[3", "2mhi"]), "hi");
    }

    #[test]
    fn scroll_bounds() {
        assert_eq!(fed(&vec!["x\n"; 24]).lines().count(), 23);
    }

    #[test]
    fn scrollback() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        grid.show = false;
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n");
        assert_eq!(grid.text(), "4\n5\n6");
        grid.wheel(2.0);
        assert_eq!(grid.text(), "2\n3\n4\n5");
        grid.wheel(-2.0);
        assert_eq!(grid.text(), "4\n5\n6");
    }

    #[test]
    fn below_caps() {
        let mut grid = Grid::new(4, 10, 1, Palette::default());
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n");
        grid.wheel(-100.0);
        assert_eq!(grid.text(), "5\n6\n");
    }

    #[test]
    fn bench_text_full_scrollback() {
        let mut grid = Grid::new(22, 104, 22, Palette::default());
        let mut parse = vte::Parser::new();
        let line = "x".repeat(104);
        for _ in 0..1100 {
            parse.advance(&mut grid, line.as_bytes());
            parse.advance(&mut grid, b"\r\n");
        }
        let now = std::time::Instant::now();
        for _ in 0..100 {
            let _ = grid.text();
        }
        eprintln!("text x100: {:?}", now.elapsed());
    }

    #[test]
    fn below_larger_than_rows() {
        let mut grid = Grid::new(4, 10, 100, Palette::default());
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n");
        grid.wheel(-100.0);
        assert_eq!(grid.text(), "\n\n\n");
    }

    #[test]
    fn scroll_past_bottom() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        grid.show = false;
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n");
        grid.wheel(-2.0);
        assert_eq!(grid.text(), "6\n\n");
        grid.wheel(-100.0);
        assert_eq!(grid.text(), "\n\n\n");
        parse.advance(&mut grid, b"7\r\n");
        assert_eq!(grid.text(), "5\n6\n7");
    }

    #[test]
    fn same_text_stays_clean() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"hi");
        assert!(grid.take_dirty());
        parse.advance(&mut grid, b"\rhi");
        assert!(!grid.take_dirty());
    }

    #[test]
    fn newline_dirties_only_on_scroll() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"hi\n");
        assert!(grid.take_dirty());
        parse.advance(&mut grid, b"\n");
        assert!(!grid.take_dirty());
    }

    #[test]
    fn clean_erase_stays_clean() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        grid.show = false;
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"hi");
        assert!(grid.take_dirty());
        parse.advance(&mut grid, b"\x1b[K");
        assert!(!grid.take_dirty());
        parse.advance(&mut grid, b"\x1b[1G\x1b[K");
        assert!(grid.take_dirty());
        assert_eq!(grid.text(), "");
    }

    #[test]
    fn trailing_paint_survives() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        grid.show = false;
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"\x1b[48;2;26;26;28mhi   ");
        let rows = grid.spans();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].len(), 1);
        assert_eq!(rows[0][0].text, "hi   ");
        assert_eq!(rows[0][0].back, Some(Color::rgb(26, 26, 28)));
    }

    #[test]
    fn wheel_keeps_fractions() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        grid.show = false;
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n");
        grid.wheel(0.6);
        assert_eq!(grid.text(), "4\n5\n6");
        grid.wheel(0.6);
        assert_eq!(grid.text(), "3\n4\n5\n6");
    }

    #[test]
    fn sgr_fg() {
        assert_eq!(tinted(&["\x1b[31mhi"]), vec![vec![("hi".into(), rgb(1))]]);
    }

    #[test]
    fn sgr_bold_brightens() {
        assert_eq!(tinted(&["\x1b[1;31mhi"]), vec![vec![("hi".into(), rgb(9))]]);
    }

    #[test]
    fn sgr_reset() {
        let fore = {
            let rgb = Palette::default().fore;
            (rgb[0], rgb[1], rgb[2])
        };
        assert_eq!(
            tinted(&["\x1b[31ma\x1b[0mb"]),
            vec![vec![("a".into(), rgb(1)), ("b".into(), fore)]]
        );
    }

    #[test]
    fn sgr_index() {
        assert_eq!(
            tinted(&["\x1b[38;5;196mx"]),
            vec![vec![("x".into(), (255, 0, 0))]]
        );
    }

    #[test]
    fn sgr_rgb() {
        assert_eq!(
            tinted(&["\x1b[38;2;1;2;3mx"]),
            vec![vec![("x".into(), (1, 2, 3))]]
        );
    }

    fn painted(chunks: &[&str]) -> Vec<Vec<(String, Option<(u8, u8, u8)>, bool, bool, usize)>> {
        let mut grid = Grid::new(24, 80, 24, Palette::default());
        grid.show = false;
        let mut parse = vte::Parser::new();
        for chunk in chunks {
            parse.advance(&mut grid, chunk.as_bytes());
        }
        grid.spans()
            .into_iter()
            .map(|line| {
                line.into_iter()
                    .map(|span| {
                        (
                            span.text,
                            span.back.map(|c| (c.r(), c.g(), c.b())),
                            span.under,
                            span.strike,
                            span.col,
                        )
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn bg_runs() {
        assert_eq!(
            painted(&["a\x1b[41mb\x1b[49mc"]),
            vec![vec![
                ("a".into(), None, false, false, 0),
                ("b".into(), Some((0xCC, 0, 0)), false, false, 1),
                ("c".into(), None, false, false, 2),
            ]]
        );
    }

    #[test]
    fn ech_erases() {
        assert_eq!(fed(&["ABC\x1b[1;1H\x1b[2X:"]), ": C");
        assert_eq!(fed(&["ABC\x1b[1;1H\x1b[23X:"]), ":");
    }

    #[test]
    fn private_sgr_ignored() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        grid.show = false;
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"\x1b[>4;2mhi");
        assert!(!grid.spans()[0][0].under);
    }

    #[test]
    fn deco_flags() {
        assert_eq!(
            painted(&["\x1b[4mu\x1b[24m \x1b[9ms\x1b[29m"]),
            vec![vec![
                ("u".into(), None, true, false, 0),
                (" ".into(), None, false, false, 1),
                ("s".into(), None, false, true, 2),
            ]]
        );
    }

    #[test]
    fn alt_swaps_and_restores() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"main\x1b[?1049h");
        assert_eq!(grid.text(), "\u{2588}");
        parse.advance(&mut grid, b"vim");
        assert_eq!(grid.text(), "vim\u{2588}");
        parse.advance(&mut grid, b"\x1b[?1049l");
        assert_eq!(grid.text(), "main\u{2588}");
    }

    #[test]
    fn alt_has_no_scrollback() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"\x1b[?1049h1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n");
        grid.wheel(2.0);
        grid.show = false;
        assert_eq!(grid.text(), "4\n5\n6");
    }

    #[test]
    fn cursor_block() {
        let fore = {
            let rgb = Palette::default().fore;
            (rgb[0], rgb[1], rgb[2])
        };
        let mut grid = Grid::new(24, 80, 24, Palette::default());
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"hi");
        let line = &grid.spans()[0];
        assert_eq!(line.len(), 1);
        assert_eq!(line[0].text, "hi\u{2588}");
        let hue = line[0].hue;
        assert_eq!((hue.r(), hue.g(), hue.b()), fore);
    }

    #[test]
    fn cursor_hides() {
        assert_eq!(fed(&["\x1b[?25lhi"]), "hi");
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n");
        grid.wheel(1.0);
        assert!(!grid.text().contains('\u{2588}'));
    }

    #[test]
    fn resize_clears_cells() {        let mut grid = Grid::new(4, 10, 4, Palette::default());
        grid.show = false;
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"hi");
        grid.resize(2, 5);
        assert_eq!(grid.text(), "");
        parse.advance(&mut grid, b"abc");
        assert_eq!(grid.text(), "abc");
    }
}

    #[test]
    fn title_sets() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"\x1b]0;work\x07");
        assert_eq!(grid.take_title().as_deref(), Some("work"));
        assert!(grid.take_title().is_none());
        parse.advance(&mut grid, b"\x1b]2;icon+title\x07");
        assert_eq!(grid.take_title().as_deref(), Some("icon+title"));
    }

    #[test]
    fn paste_tracks_mode() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        let mut parse = vte::Parser::new();
        assert!(!grid.pastes());
        parse.advance(&mut grid, b"\x1b[?2004h");
        assert!(grid.pastes());
        parse.advance(&mut grid, b"\x1b[?2004l");
        assert!(!grid.pastes());
    }

    #[test]
    fn blink_tracks_shape() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"\x1b[1 q");
        assert!(grid.blinked());
        parse.advance(&mut grid, b"\x1b[2 q");
        assert!(!grid.blinked());
        parse.advance(&mut grid, b"\x1b[0 q");
        assert!(grid.blinked());
    }

    #[test]
    fn phase_hides_cursor() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"\x1b[1 q");
        assert!(grid.spans()[0][0].text.starts_with('█'));
        grid.flip();
        assert_eq!(grid.spans()[0][0].text, " ");
        grid.flip();
        assert!(grid.spans()[0][0].text.starts_with('█'));
    }

    #[test]
    fn cursor_shapes() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"\x1b[3 q");
        assert!(grid.spans()[0][0].text.starts_with('▁'));
        parse.advance(&mut grid, b"\x1b[5 q");
        assert!(grid.spans()[0][0].text.starts_with('▏'));
        parse.advance(&mut grid, b"\x1b[0 q");
        assert!(grid.spans()[0][0].text.starts_with('█'));
    }

    #[test]
    fn focus_reports() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"\x1b[?1004h");
        assert!(grid.focused());
        grid.focus(true);
        assert_eq!(grid.take_reply(), b"\x1b[I");
        grid.focus(false);
        assert_eq!(grid.take_reply(), b"\x1b[O");
        parse.advance(&mut grid, b"\x1b[?1004l");
        assert!(!grid.focused());
    }

    #[test]
    fn bold_sets_weight() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
        grid.show = false;
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"\x1b[1mhi\x1b[22mlo");
        let spans = grid.spans();
        assert!(spans[0][0].bold);
        assert_eq!(spans[0][0].text, "hi");
        assert!(!spans[0][1].bold);
    }
