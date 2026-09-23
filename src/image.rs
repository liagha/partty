use base64::prelude::*;
use std::collections::{HashMap, VecDeque};

pub struct Cut {
    buf: Vec<u8>,
    trail: bool,
}

impl Cut {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            trail: false,
        }
    }

    pub fn split(&mut self, bytes: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
        let mut data = Vec::with_capacity(self.buf.len() + bytes.len() + 1);
        data.extend_from_slice(&self.buf);
        self.buf.clear();
        if self.trail {
            self.trail = false;
            data.push(0x1b);
        }
        data.extend_from_slice(bytes);
        let mut clean = Vec::with_capacity(data.len());
        let mut out = Vec::new();
        let mut i = 0;
        while i < data.len() {
            if data[i] == 0x1b && i + 1 < data.len() && data[i + 1] == b'_' {
                match find_st(&data, i + 2) {
                    Some(end) => {
                        if data[i + 2] == b'G' {
                            out.push(data[i + 2..end].to_vec());
                        }
                        i = end + 2;
                    }
                    None => {
                        self.buf.extend_from_slice(&data[i..]);
                        break;
                    }
                }
            } else if data[i] == 0x1b && i + 1 == data.len() {
                self.trail = true;
                break;
            } else {
                clean.push(data[i]);
                i += 1;
            }
        }
        (clean, out)
    }
}

fn find_st(data: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 1 < data.len() {
        if data[i] == 0x1b && data[i + 1] == b'\\' {
            return Some(i);
        }
        i += 1;
    }
    None
}

pub struct Entry {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

pub struct Store {
    map: HashMap<u32, Entry>,
    order: VecDeque<u32>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn put(&mut self, id: u32, entry: Entry) -> Option<u32> {
        if self.map.contains_key(&id) {
            self.order.retain(|&x| x != id);
        }
        self.order.push_back(id);
        self.map.insert(id, entry);
        let mut lost = None;
        while self.map.len() > 64 {
            match self.order.pop_front() {
                Some(old) => {
                    if self.map.remove(&old).is_some() {
                        lost = Some(old);
                    }
                }
                None => break,
            }
        }
        lost
    }

    pub fn get(&self, id: u32) -> Option<&Entry> {
        self.map.get(&id)
    }

    pub fn free(&mut self, id: u32) {
        self.map.remove(&id);
        self.order.retain(|&x| x != id);
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }
}

pub struct Ctl {
    pub act: u8,
    pub id: u32,
    pub fmt: u8,
    pub img_w: u32,
    pub img_h: u32,
    pub cols: u32,
    pub rows: u32,
    pub still: bool,
    pub z: i32,
    pub more: bool,
}

fn num(raw: &[u8]) -> Option<u32> {
    std::str::from_utf8(raw).ok()?.parse().ok()
}

pub fn kitty(payload: &[u8]) -> Option<(Ctl, Vec<u8>)> {
    let at = payload.iter().position(|&b| b == b';')?;
    let (head, data) = payload.split_at(at);
    if head.first() != Some(&b'G') {
        return None;
    }
    let mut ctl = Ctl {
        act: b'T',
        id: 0,
        fmt: 32,
        img_w: 0,
        img_h: 0,
        cols: 0,
        rows: 0,
        still: false,
        z: 0,
        more: false,
    };
    for kv in head[1..].split(|&b| b == b',') {
        let mut pair = kv.splitn(2, |&b| b == b'=');
        let (key, val) = (pair.next()?, pair.next()?);
        match key {
            b"a" => ctl.act = val.first().copied().unwrap_or(b'T'),
            b"i" => ctl.id = num(val)?,
            b"f" => ctl.fmt = num(val)? as u8,
            b"s" => ctl.img_w = num(val)?,
            b"v" => ctl.img_h = num(val)?,
            b"c" => ctl.cols = num(val)?,
            b"r" => ctl.rows = num(val)?,
            b"C" => ctl.still = val == b"1",
            b"z" => ctl.z = std::str::from_utf8(val).ok()?.parse().ok()?,
            b"m" => ctl.more = val == b"1",
            _ => {}
        }
    }
    Some((ctl, data[1..].to_vec()))
}

pub fn decode(fmt: u8, w: u32, h: u32, data: &[u8]) -> Option<Entry> {
    match fmt {
        100 => {
            let img = image::load_from_memory(data).ok()?.to_rgba8();
            let (w, h) = (img.width(), img.height());
            Some(Entry {
                w,
                h,
                rgba: img.into_raw(),
            })
        }
        32 => {
            if w == 0 || h == 0 || data.len() != w as usize * h as usize * 4 {
                return None;
            }
            Some(Entry {
                w,
                h,
                rgba: data.to_vec(),
            })
        }
        24 => {
            if w == 0 || h == 0 || data.len() != w as usize * h as usize * 3 {
                return None;
            }
            let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
            for px in data.chunks_exact(3) {
                rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            Some(Entry { w, h, rgba })
        }
        _ => None,
    }
}

pub fn cells_for(img_w: u32, img_h: u32, cell_w: f32, cell_h: f32, want_c: u32, want_r: u32) -> (u32, u32) {
    let cell_w = cell_w.max(1.0);
    let cell_h = cell_h.max(1.0);
    match (want_c, want_r) {
        (0, 0) => (
            ((img_w as f32 / cell_w).ceil() as u32).max(1),
            ((img_h as f32 / cell_h).ceil() as u32).max(1),
        ),
        (c, 0) => (
            c,
            ((img_h as f32 * c as f32 * cell_w / img_w.max(1) as f32 / cell_h).ceil() as u32)
                .max(1),
        ),
        (0, r) => (
            ((img_w as f32 * r as f32 * cell_h / img_h.max(1) as f32 / cell_w).ceil() as u32)
                .max(1),
            r,
        ),
        (c, r) => (c, r),
    }
}

pub struct Six {
    pal: [Option<(u8, u8, u8)>; 1024],
    cur: (u8, u8, u8),
    x: usize,
    y: usize,
    w: usize,
    rows: Vec<Vec<[u8; 4]>>,
}

impl Six {
    pub fn new() -> Self {
        Self {
            pal: [None; 1024],
            cur: (255, 255, 255),
            x: 0,
            y: 0,
            w: 0,
            rows: Vec::new(),
        }
    }

    pub fn plot(&mut self, x: usize, y: usize) {
        while self.rows.len() <= y {
            self.rows.push(Vec::new());
        }
        let row = &mut self.rows[y];
        while row.len() <= x {
            row.push([0, 0, 0, 0]);
        }
        let (r, g, b) = self.cur;
        row[x] = [r, g, b, 255];
        self.w = self.w.max(x + 1);
    }

    pub fn put(&mut self, data: &[u8]) {
        let mut i = 0;
        while i < data.len() {
            match data[i] {
                b'#' => {
                    let (all, n) = ints(&data[i + 1..]);
                    i += 1 + n;
                    match all.as_slice() {
                        [r, 2, x, y, z] => {
                            let px = |v: u16| (v.min(100) as u32 * 255 / 100) as u8;
                            let col = (px(*x), px(*y), px(*z));
                            if (*r as usize) < 1024 {
                                self.pal[*r as usize] = Some(col);
                            }
                            self.cur = col;
                        }
                        [r, ..] => {
                            if (*r as usize) < 1024 {
                                self.cur = self.pal[*r as usize].unwrap_or((255, 255, 255));
                            }
                        }
                        [] => {}
                    }
                }
                b'$' => {
                    self.x = 0;
                    i += 1;
                }
                b'-' => {
                    self.y += 6;
                    self.x = 0;
                    i += 1;
                }
                0x3f..=0x7e => {
                    let bits = data[i] - 0x3f;
                    for k in 0..6 {
                        if bits & (1 << k) != 0 {
                            self.plot(self.x, self.y + k);
                        }
                    }
                    self.x += 1;
                    i += 1;
                }
                _ => i += 1,
            }
        }
    }

    pub fn finish(self) -> Option<Entry> {
        if self.w == 0 || self.rows.is_empty() {
            return None;
        }
        let h = self.rows.len();
        let mut rgba = Vec::with_capacity(self.w * h * 4);
        for mut row in self.rows {
            row.resize(self.w, [0, 0, 0, 0]);
            for px in row {
                rgba.extend_from_slice(&px);
            }
        }
        Some(Entry {
            w: self.w as u32,
            h: h as u32,
            rgba,
        })
    }
}

fn ints(data: &[u8]) -> (Vec<u16>, usize) {
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        let mut n: u16 = 0;
        let mut len = 0;
        while i < data.len() && data[i].is_ascii_digit() {
            n = n.saturating_mul(10).saturating_add((data[i] - b'0') as u16);
            len += 1;
            i += 1;
        }
        if len == 0 {
            break;
        }
        out.push(n);
        if data.get(i) == Some(&b';') {
            i += 1;
        } else {
            break;
        }
    }
    (out, i)
}

pub struct File {
    pub cols: u32,
    pub rows: u32,
    pub png: Vec<u8>,
}

pub fn file(head: &[u8], data: &[u8]) -> Option<File> {
    let mut cols = 0;
    let mut rows = 0;
    for kv in head.split(|&b| b == b';') {
        let mut pair = kv.splitn(2, |&b| b == b'=');
        let (key, val) = (pair.next()?, pair.next()?);
        match key {
            b"width" => cols = cells(val),
            b"height" => rows = cells(val),
            _ => {}
        }
    }
    let mut raw: Vec<u8> = data.iter().copied().filter(|b| !b.is_ascii_whitespace()).collect();
    while raw.len() % 4 == 1 {
        raw.pop();
    }
    Some(File {
        cols,
        rows,
        png: BASE64_STANDARD.decode(&raw).ok()?,
    })
}

fn cells(raw: &[u8]) -> u32 {
    if raw.iter().all(|b| b.is_ascii_digit()) && !raw.is_empty() {
        std::str::from_utf8(raw).ok().and_then(|s| s.parse().ok()).unwrap_or(0)
    } else {
        0
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn apc(payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0x1b, b'_'];
        out.extend_from_slice(payload);
        out.extend_from_slice(&[0x1b, b'\\']);
        out
    }

    #[test]
    fn cut_pulls_apc() {
        let mut cut = Cut { buf: Vec::new(), trail: false };
        let mut bytes = b"hi ".to_vec();
        bytes.extend(apc(b"G1=2;OK"));
        bytes.extend(b" yo");
        let (clean, out) = cut.split(&bytes);
        assert_eq!(clean, b"hi  yo");
        assert_eq!(out, vec![b"G1=2;OK".to_vec()]);
    }

    #[test]
    fn cut_holds_chunks() {
        let mut cut = Cut { buf: Vec::new(), trail: false };
        let full = apc(b"Ga=T,f=100;QUJD");
        let (clean, out) = cut.split(&full[..6]);
        assert!(clean.is_empty() && out.is_empty());
        let (clean, out) = cut.split(&full[6..]);
        assert!(clean.is_empty());
        assert_eq!(out, vec![b"Ga=T,f=100;QUJD".to_vec()]);
    }

    #[test]
    fn cut_esc_tail() {
        let mut cut = Cut { buf: Vec::new(), trail: false };
        let (clean, _) = cut.split(b"a\x1b");
        assert_eq!(clean, b"a");
        let (clean, _) = cut.split(b"[2J");
        assert_eq!(clean, b"\x1b[2J");
    }

    #[test]
    fn cut_drops_other() {
        let mut cut = Cut { buf: Vec::new(), trail: false };
        let mut bytes = b"x".to_vec();
        bytes.extend(apc(b"Qsomething"));
        bytes.extend(b"y");
        let (clean, out) = cut.split(&bytes);
        assert_eq!(clean, b"xy");
        assert!(out.is_empty());
    }

    #[test]
    fn kitty_parses() {
        let (ctl, data) = kitty(b"Ga=T,f=100,m=1;QUJD").unwrap();
        assert_eq!((ctl.act, ctl.fmt, ctl.id), (b'T', 100, 0));
        assert!(ctl.more);
        assert_eq!(data, b"QUJD");
        assert!(kitty(b"nope").is_none());
    }

    #[test]
    fn decode_raw() {
        let rgba = vec![10, 20, 30, 40, 50, 60, 70, 80];
        let e = decode(32, 2, 1, &rgba).unwrap();
        assert_eq!((e.w, e.h), (2, 1));
        assert_eq!(e.rgba, rgba);
        let rgb = vec![10, 20, 30];
        let e = decode(24, 1, 1, &rgb).unwrap();
        assert_eq!(e.rgba, vec![10, 20, 30, 255]);
        assert!(decode(32, 2, 1, &[1, 2]).is_none());
        assert!(decode(7, 1, 1, &[1]).is_none());
    }

    #[test]
    fn decode_png() {
        let img = image::RgbaImage::from_pixel(2, 3, image::Rgba([9, 8, 7, 255]));
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        let e = decode(100, 0, 0, &buf).unwrap();
        assert_eq!((e.w, e.h), (2, 3));
        assert_eq!(&e.rgba[..4], &[9, 8, 7, 255]);
    }

    #[test]
    fn sixel_red() {
        let mut six = Six {
            pal: [None; 1024],
            cur: (0, 0, 0),
            x: 0,
            y: 0,
            w: 0,
            rows: Vec::new(),
        };
        six.put(b"#0;2;100;0;0#0~");
        let e = six.finish().unwrap();
        assert_eq!((e.w, e.h), (1, 6));
        assert_eq!(&e.rgba[..4], &[255, 0, 0, 255]);
        assert_eq!(&e.rgba[20..24], &[255, 0, 0, 255]);
    }

    #[test]
    fn file_cells() {
        let f = file(b"name=x;width=10;height=4", b"aGk=").unwrap();
        assert_eq!((f.cols, f.rows), (10, 4));
        assert_eq!(f.png, b"hi");
        let f = file(b"width=10px", b"aGk=").unwrap();
        assert_eq!((f.cols, f.rows), (0, 0));
    }

    #[test]
    fn geometry() {
        assert_eq!(cells_for(100, 50, 10.0, 10.0, 0, 0), (10, 5));
        assert_eq!(cells_for(100, 50, 10.0, 10.0, 4, 0), (4, 2));
        assert_eq!(cells_for(100, 50, 10.0, 10.0, 0, 2), (4, 2));
        assert_eq!(cells_for(100, 50, 10.0, 10.0, 3, 2), (3, 2));
    }

    fn entry() -> Entry {
        Entry {
            w: 1,
            h: 1,
            rgba: vec![1, 2, 3, 255],
        }
    }

    #[test]
    fn store_evicts_oldest() {
        let mut store = Store::new();
        for id in 1..=65u32 {
            store.put(id, entry());
        }
        assert!(store.get(1).is_none());
        assert!(store.get(65).is_some());
    }

    #[test]
    fn store_free_cleans_order() {
        let mut store = Store::new();
        for id in 1..=64u32 {
            store.put(id, entry());
        }
        store.free(1);
        for id in 65..=128u32 {
            store.put(id, entry());
        }
        assert!(store.get(2).is_none());
        assert!(store.get(128).is_some());
    }

    #[test]
    fn store_refreshes_reused() {
        let mut store = Store::new();
        for id in 1..=64u32 {
            store.put(id, entry());
        }
        store.put(1, entry());
        store.put(65, entry());
        assert!(store.get(1).is_some());
        assert!(store.get(2).is_none());
    }
}
