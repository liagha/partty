use serde::Deserialize;

pub use glyphon::Color;

pub const FORE: [u8; 3] = [0xE6, 0xE6, 0xEB];
pub const BACK: [u8; 3] = [0x4B, 0x4B, 0x54];

pub fn hex(text: &str) -> Option<[u8; 3]> {
    let text = text.strip_prefix('#').unwrap_or(text);
    if text.len() != 6 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(text, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

#[derive(Deserialize, Default)]
pub struct Colors {
    pub foreground: Option<String>,
    pub black: Option<String>,
    pub red: Option<String>,
    pub green: Option<String>,
    pub yellow: Option<String>,
    pub blue: Option<String>,
    pub magenta: Option<String>,
    pub cyan: Option<String>,
    pub white: Option<String>,
    pub bright_black: Option<String>,
    pub bright_red: Option<String>,
    pub bright_green: Option<String>,
    pub bright_yellow: Option<String>,
    pub bright_blue: Option<String>,
    pub bright_magenta: Option<String>,
    pub bright_cyan: Option<String>,
    pub bright_white: Option<String>,
}

pub struct Palette {
    pub slots: [[u8; 3]; 16],
    pub fore: [u8; 3],
    pub back: [u8; 3],
}

impl Palette {
    pub fn default() -> Self {
        Self {
            slots: [
                [0x2E, 0x34, 0x36],
                [0xCC, 0x00, 0x00],
                [0x4E, 0x9A, 0x06],
                [0xC4, 0xA0, 0x00],
                [0x34, 0x65, 0xA4],
                [0x75, 0x50, 0x7B],
                [0x06, 0x98, 0x9A],
                [0xD3, 0xD7, 0xCF],
                [0x55, 0x57, 0x53],
                [0xEF, 0x29, 0x29],
                [0x8A, 0xE2, 0x34],
                [0xFC, 0xE9, 0x4F],
                [0x72, 0x9F, 0xCF],
                [0xAD, 0x7F, 0xA8],
                [0x34, 0xE2, 0xE2],
                [0xEE, 0xEE, 0xEC],
            ],
            fore: FORE,
            back: BACK,
        }
    }

    pub fn load(&mut self, over: &Colors) {
        Self::set(&mut self.fore, &over.foreground);
        let s = &mut self.slots;
        Self::set(&mut s[0], &over.black);
        Self::set(&mut s[1], &over.red);
        Self::set(&mut s[2], &over.green);
        Self::set(&mut s[3], &over.yellow);
        Self::set(&mut s[4], &over.blue);
        Self::set(&mut s[5], &over.magenta);
        Self::set(&mut s[6], &over.cyan);
        Self::set(&mut s[7], &over.white);
        Self::set(&mut s[8], &over.bright_black);
        Self::set(&mut s[9], &over.bright_red);
        Self::set(&mut s[10], &over.bright_green);
        Self::set(&mut s[11], &over.bright_yellow);
        Self::set(&mut s[12], &over.bright_blue);
        Self::set(&mut s[13], &over.bright_magenta);
        Self::set(&mut s[14], &over.bright_cyan);
        Self::set(&mut s[15], &over.bright_white);
    }

    fn set(slot: &mut [u8; 3], text: &Option<String>) {
        if let Some(text) = text {
            if let Some(rgb) = hex(text) {
                *slot = rgb;
            }
        }
    }

    pub fn dye(&self, i: u8) -> Color {
        let rgb = match i {
            0..=15 => self.slots[i as usize],
            16..=231 => {
                let j = i - 16;
                let to = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
                [to(j / 36), to((j % 36) / 6), to(j % 6)]
            }
            _ => {
                let v = 8 + (i - 232) * 10;
                [v, v, v]
            }
        };
        Color::rgb(rgb[0], rgb[1], rgb[2])
    }

    pub fn fore(&self) -> Color {
        Color::rgb(self.fore[0], self.fore[1], self.fore[2])
    }

    pub fn back(&self) -> Color {
        Color::rgb(self.back[0], self.back[1], self.back[2])
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn hex_valid() {
        assert_eq!(hex("#4b4b54"), Some([0x4B, 0x4B, 0x54]));
        assert_eq!(hex("ff0000"), Some([255, 0, 0]));
    }

    #[test]
    fn hex_bad() {
        assert_eq!(hex("red"), None);
        assert_eq!(hex("#12345"), None);
        assert_eq!(hex("#zzzzzz"), None);
    }

    #[test]
    fn dye_cube() {
        let inks = Palette::default();
        assert_eq!(inks.dye(196), Color::rgb(255, 0, 0));
        assert_eq!(inks.dye(231), Color::rgb(255, 255, 255));
        assert_eq!(inks.dye(232), Color::rgb(8, 8, 8));
        assert_eq!(inks.dye(255), Color::rgb(238, 238, 238));
        assert_eq!(inks.dye(1), Color::rgb(0xCC, 0, 0));
    }

    #[test]
    fn load_override() {
        let mut inks = Palette::default();
        let over: Colors = toml::from_str("red = \"#ff0000\"\nbright_blue = \"nope\"\n").unwrap();
        inks.load(&over);
        assert_eq!(inks.slots[1], [255, 0, 0]);
        assert_eq!(inks.slots[12], [0x72, 0x9F, 0xCF]);
    }
}
