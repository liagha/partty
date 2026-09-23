# partty

A GPU terminal emulator with proper Persian and RTL text.

## Features

- Persian, Arabic, and bidirectional text
- Truecolor, 256 colors, and 16-color palette
- Scrollback with mouse wheel scrolling
- Alt-screen apps (vim, less, htop, btop)
- Mouse reporting and clipboard
- Kitty, Sixel, and iTerm images
- Blinking block, bar, and underline cursor
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
red = "#cc0000"
```

Missing file or keys fall back to defaults.
