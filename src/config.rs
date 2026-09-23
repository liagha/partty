use serde::Deserialize;

use crate::color::{self, Colors, Palette};
use crate::text::{FACE, SIZE};

#[derive(Deserialize, Default)]
struct File {
    background: Option<String>,
    foreground: Option<String>,
    past_bottom: Option<usize>,
    font_size: Option<f32>,
    mono: Option<String>,
    fonts: Option<Vec<String>>,
    colors: Option<Colors>,
}

pub struct Config {
    pub bg: [f64; 3],
    pub below: Option<usize>,
    pub size: f32,
    pub face: String,
    pub files: Vec<std::path::PathBuf>,
    pub inks: Palette,
}

impl Config {
    const FALLBACK: &str = "#1e1e1e";

    pub fn load() -> (Self, Option<std::path::PathBuf>) {
        if let Some(found) = Self::path() {
            return (Self::load_from(&found), Some(found));
        }
        (Self::parse(""), None)
    }

    fn path() -> Option<std::path::PathBuf> {
        let local = std::path::PathBuf::from("config.toml");
        if local.is_file() {
            return Some(local);
        }
        let file = Self::dir()?.join("config.toml");
        file.is_file().then_some(file)
    }

    #[cfg(target_os = "windows")]
    fn dir() -> Option<std::path::PathBuf> {
        std::env::var_os("APPDATA").map(|base| std::path::PathBuf::from(base).join("partty"))
    }

    #[cfg(target_os = "macos")]
    fn dir() -> Option<std::path::PathBuf> {
        std::env::var_os("HOME")
            .map(|home| std::path::PathBuf::from(home).join("Library/Application Support/partty"))
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    fn dir() -> Option<std::path::PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
            })
            .map(|base| base.join("partty"))
    }

    pub fn load_from(path: &std::path::Path) -> Self {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        Self::parse(&text)
    }

    fn parse(text: &str) -> Self {
        let file: File = toml::from_str(text).unwrap_or_default();
        let back = file
            .background
            .as_deref()
            .and_then(color::hex)
            .unwrap_or_else(|| color::hex(Self::FALLBACK).unwrap_or(color::BACK));
        let fore = file
            .foreground
            .as_deref()
            .and_then(color::hex)
            .unwrap_or(color::FORE);
        let mut inks = Palette::default();
        inks.back = back;
        inks.fore = fore;
        if let Some(over) = &file.colors {
            inks.load(over);
        }
        Self {
            bg: Self::linear(back),
            below: file.past_bottom,
            size: file.font_size.filter(|s| (8.0..=96.0).contains(s)).unwrap_or(SIZE),
            face: file.mono.unwrap_or_else(|| FACE.into()),
            files: file.fonts.unwrap_or_default().into_iter().map(Into::into).collect(),
            inks,
        }
    }

    fn linear(rgb: [u8; 3]) -> [f64; 3] {
        let to = |byte: u8| {
            let v = byte as f64 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        [to(rgb[0]), to(rgb[1]), to(rgb[2])]
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        a.iter()
            .zip(b.iter())
            .all(|(x, y)| (x - y).abs() < 0.002)
    }

    #[test]
    fn load_from_reads_file() {
        let path = std::env::temp_dir().join("partty-test-config.toml");
        std::fs::write(&path, "font_size = 20\n").unwrap();
        let config = Config::load_from(&path);
        std::fs::remove_file(&path).ok();
        assert_eq!(config.size, 20.0);
    }

    #[test]
    fn parse_full() {
        let config = Config::parse("background = \"#000000\"\npast_bottom = 5\n");
        assert!(close(config.bg, [0.0, 0.0, 0.0]));
        assert_eq!(config.below, Some(5));
    }

    #[test]
    fn font_size_bounds() {
        assert_eq!(Config::parse("font_size = 20\n").size, 20.0);
        assert_eq!(Config::parse("font_size = 200\n").size, SIZE);
        assert_eq!(Config::parse("").size, SIZE);
    }

    #[test]
    fn parse_empty() {
        let config = Config::parse("");
        assert!(close(
            config.bg,
            Config::linear(color::hex(Config::FALLBACK).unwrap())
        ));
        assert_eq!(config.below, None);
    }

    #[test]
    fn parse_broken() {
        let broken = Config::parse("[[[");
        let empty = Config::parse("");
        assert!(close(broken.bg, empty.bg));
        assert_eq!(broken.below, None);
        let bad = Config::parse("background = \"nope\"\n");
        assert!(close(bad.bg, empty.bg));
    }

    #[test]
    fn parse_mono() {
        let config = Config::parse("mono = \"JetBrains Mono\"\nfonts = [\"/a.ttf\"]\n");
        assert_eq!(config.face, "JetBrains Mono");
        assert_eq!(config.files, vec![std::path::PathBuf::from("/a.ttf")]);
        let empty = Config::parse("");
        assert_eq!(empty.face, FACE);
        assert!(empty.files.is_empty());
    }

    #[test]
    fn parse_colors() {
        let config = Config::parse("foreground = \"#ffffff\"\n[colors]\nred = \"#ff0000\"\n");
        assert_eq!(config.inks.fore, [255, 255, 255]);
        assert_eq!(config.inks.slots[1], [255, 0, 0]);
    }
}
