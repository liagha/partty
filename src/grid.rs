use std::collections::VecDeque;
use vte::{Params, Perform};

use crate::color::{Color, Palette};

const PAST: usize = 1000;

#[derive(Clone, Copy, PartialEq)]
struct Cell {
    glyph: char,
    fg: Color,
    bg: Color,
}

struct Pen {
    fg: Color,
    bg: Color,
    bold: bool,
    slot: Option<u8>,
}

impl Pen {
    fn of(inks: &Palette) -> Self {
        Self {
            fg: inks.fore(),
            bg: inks.back(),
            bold: false,
            slot: None,
        }
    }
}

pub struct Span {
    pub hue: Color,
    pub text: String,
}

pub struct Grid {
    cells: Vec<Vec<Cell>>,
    past: VecDeque<Vec<Cell>>,
    off: i32,
    carry: f32,
    below: usize,
    dirty: bool,
    pen: Pen,
    inks: Palette,
    rows: usize,
    cols: usize,
    row: usize,
    col: usize,
}

impl Grid {
    pub fn new(rows: usize, cols: usize, below: usize, inks: Palette) -> Self {
        let mut grid = Self {
            cells: vec![],
            past: VecDeque::new(),
            off: 0,
            carry: 0.0,
            below,
            dirty: false,
            pen: Pen::of(&inks),
            inks,
            rows: 0,
            cols: 0,
            row: 0,
            col: 0,
        };
        grid.resize(rows, cols);
        grid
    }

    fn empty(&self) -> Cell {
        Cell {
            glyph: ' ',
            fg: self.pen.fg,
            bg: self.pen.bg,
        }
    }

    pub fn resize(&mut self, rows: usize, cols: usize) {
        let fill = self.empty();
        self.cells = vec![vec![fill; cols]; rows];
        self.rows = rows;
        self.cols = cols;
        self.row = 0;
        self.col = 0;
        self.dirty = true;
        self.off = self
            .off
            .clamp(-(self.below as i32), self.past.len() as i32);
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::replace(&mut self.dirty, false)
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
        self.carry += lines;
        let whole = self.carry.trunc() as i32;
        self.carry -= whole as f32;
        let off = self.off + whole;
        let clamped = off.clamp(-(self.below as i32), self.past.len() as i32);
        self.dirty = clamped != self.off;
        self.off = clamped;
    }

    fn scroll(&mut self) {
        let top = self.cells.remove(0);
        self.past.push_back(top);
        if self.past.len() > PAST {
            self.past.pop_front();
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

    fn down(&mut self) -> bool {
        if self.row + 1 >= self.rows {
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
        let total = self.past.len() + self.rows;
        let end = total as i32 - self.off;
        let stop = end.clamp(0, total as i32) as usize;
        let start = (end - self.rows as i32).max(0).min(stop as i32) as usize;
        let row = |i: usize| {
            if i < self.past.len() {
                &self.past[i]
            } else {
                &self.cells[i - self.past.len()]
            }
        };
        let mut out: Vec<Vec<Span>> = (start..stop).map(|i| self.runs(row(i))).collect();
        while out.last().is_some_and(|line| line.is_empty()) {
            out.pop();
        }
        out.extend(
            std::iter::repeat_with(Vec::new)
                .take((-self.off).max(0).min(self.rows as i32) as usize),
        );
        out
    }

    fn runs(&self, row: &Vec<Cell>) -> Vec<Span> {
        let mut end = row.len();
        while end > 0 && row[end - 1].glyph == ' ' {
            end -= 1;
        }
        let mut spans: Vec<Span> = Vec::new();
        for cell in &row[..end] {
            match spans.last_mut() {
                Some(span) if span.hue == cell.fg => span.text.push(cell.glyph),
                _ => spans.push(Span {
                    hue: cell.fg,
                    text: cell.glyph.to_string(),
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
        let fill = Cell {
            glyph: c,
            fg: self.pen.fg,
            bg: self.pen.bg,
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

    fn csi_dispatch(&mut self, params: &Params, _mid: &[u8], _ignore: bool, action: char) {
        let arg = |i: usize, default: u16| {
            params
                .iter()
                .nth(i)
                .and_then(|group| group.first().copied())
                .unwrap_or(default)
        };
        let maxr = self.rows - 1;
        let maxc = self.cols - 1;
        let edge = self.col.min(maxc);
        match action {
            'A' => self.row = self.row.saturating_sub(arg(0, 1) as usize),
            'B' => self.row = (self.row + arg(0, 1) as usize).min(maxr),
            'C' => self.col = (self.col + arg(0, 1) as usize).min(maxc),
            'D' => self.col = self.col.saturating_sub(arg(0, 1) as usize),
            'E' => {
                self.row = (self.row + arg(0, 1) as usize).min(maxr);
                self.col = 0;
            }
            'F' => {
                self.row = self.row.saturating_sub(arg(0, 1) as usize);
                self.col = 0;
            }
            'G' => self.col = (arg(0, 1) as usize).saturating_sub(1).min(maxc),
            'H' | 'f' => {
                self.row = (arg(0, 1) as usize).saturating_sub(1).min(maxr);
                self.col = (arg(1, 1) as usize).saturating_sub(1).min(maxc);
            }
            'd' => self.row = (arg(0, 1) as usize).saturating_sub(1).min(maxr),
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
            'm' => self.sgr(params),
            _ => {}
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
        let mut parse = vte::Parser::new();
        for chunk in chunks {
            parse.advance(&mut grid, chunk.as_bytes());
        }
        grid.text()
    }

    fn tinted(chunks: &[&str]) -> Vec<Vec<(String, (u8, u8, u8))>> {
        let mut grid = Grid::new(24, 80, 24, Palette::default());
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
    fn wheel_keeps_fractions() {
        let mut grid = Grid::new(4, 10, 4, Palette::default());
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

    #[test]
    fn resize_clears_cells() {        let mut grid = Grid::new(4, 10, 4, Palette::default());
        let mut parse = vte::Parser::new();
        parse.advance(&mut grid, b"hi");
        grid.resize(2, 5);
        assert_eq!(grid.text(), "");
        parse.advance(&mut grid, b"abc");
        assert_eq!(grid.text(), "abc");
    }
}
