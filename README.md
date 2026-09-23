# partty

A GPU terminal emulator with proper Persian and RTL text.

## Features

- Persian, Arabic, and bidirectional text with Vazirmatn
- Truecolor, 256 colors, and 16-color palette
- Scrollback (1000 lines) with mouse wheel scrolling
- Alt-screen apps (vim, less, htop, btop)
- Mouse reporting, selection, and clipboard
- Blinking block, bar, and underline cursor
- Configurable fonts, colors, and font size
- Runs on Linux, macOS, and Windows

## Build

```bash
cargo build --release
./target/release/partty
```

## Config

`./config.toml`, or `~/.config/partty/config.toml`:

```toml
background = "#1e1e1e"
foreground = "#eeeeec"
font_size = 24
past_bottom = 15
mono = "JetBrains Mono"
fonts = ["~/.fonts/Custom.ttf"]

[colors]
black = "#2e3436"
red = "#cc0000"
```

| Key          | What it does                              |
| ------------ | ----------------------------------------- |
| `background` | Window background, hex                    |
| `foreground` | Default text color, hex                   |
| `font_size`  | Font size in pixels (8–96)                |
| `past_bottom`| Blank lines scrollable past the prompt    |
| `mono`       | Monospace font family                     |
| `fonts`      | Extra font files to load                  |
| `[colors]`   | Any of the 16 ansi names, hex             |

Missing file or keys fall back to defaults.

## Keys

| Keys               | Action              |
| ------------------ | ------------------- |
| `Ctrl+Shift+C`     | Copy selection      |
| `Ctrl+Shift+V`     | Paste               |
| Middle click       | Paste primary       |
| Drag               | Select (auto-copies on release) |
| Scroll wheel       | Scrollback          |

## Fonts

Ships with Vazirmatn (Persian and Latin) and DejaVu Sans Mono
(symbols and box drawing). If present on the system, Noto Emoji,
Noto CJK, and Noto Symbols are loaded automatically for emoji,
CJK, and braille graphs.
