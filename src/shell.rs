use std::io::{Read, Write};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use winit::event::ElementState;
use winit::event_loop::EventLoopProxy;
use winit::keyboard::{Key, NamedKey};

pub struct Shell {
    writer: Box<dyn std::io::Write + Send>,
    _child: Box<dyn portable_pty::Child + Send + Sync>,
    _pair: portable_pty::PtyPair,
}

impl Shell {
    fn size(rows: usize, cols: usize) -> portable_pty::PtySize {
        portable_pty::PtySize {
            rows: rows.min(u16::MAX as usize) as u16,
            cols: cols.min(u16::MAX as usize) as u16,
            pixel_width: 0,
            pixel_height: 0,
        }
    }

    pub fn open(
        rows: usize,
        cols: usize,
        wake: EventLoopProxy<()>,
    ) -> (Self, Receiver<Vec<u8>>) {
        let system = portable_pty::native_pty_system();
        let pair = system.openpty(Self::size(rows, cols)).unwrap();
        let fallback = if cfg!(target_os = "windows") {
            "powershell.exe"
        } else {
            "bash"
        };
        let name = std::env::var("SHELL").unwrap_or_else(|_| fallback.into());
        let mut cmd = portable_pty::CommandBuilder::new(name);
        cmd.env("TERM", "xterm-256color");
        let child = pair.slave.spawn_command(cmd).unwrap();
        let mut reader = pair.master.try_clone_reader().unwrap();
        let writer = pair.master.take_writer().unwrap();
        let (send, recv) = channel();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if send.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                        let _ = wake.send_event(());
                    }
                }
            }
        });
        (
            Self {
                writer,
                _child: child,
                _pair: pair,
            },
            recv,
        )
    }

    pub fn write(&mut self, bytes: &[u8]) {
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }

    pub fn resize(&self, rows: usize, cols: usize) -> Result<(), String> {
        self._pair
            .master
            .resize(Self::size(rows, cols))
            .map_err(|e| e.to_string())
    }

    pub fn key(
        state: ElementState,
        logical: &Key,
        text: Option<&str>,
        ctrl: bool,
        alt: bool,
    ) -> Option<Vec<u8>> {
        if state != ElementState::Pressed {
            return None;
        }
        if ctrl {
            let mut code = match logical {
                Key::Character(c) => Self::control(c)?,
                Key::Named(key) => Self::modified(key)?,
                _ => return None,
            };
            if alt {
                code.insert(0, 0x1b);
            }
            return Some(code);
        }
        let mut base = match logical {
            Key::Named(NamedKey::Enter) => b"\r".to_vec(),
            Key::Named(NamedKey::Backspace) => b"\x7f".to_vec(),
            Key::Named(NamedKey::Tab) => b"\t".to_vec(),
            Key::Named(NamedKey::Escape) => b"\x1b".to_vec(),
            Key::Named(NamedKey::ArrowUp) => b"\x1b[A".to_vec(),
            Key::Named(NamedKey::ArrowDown) => b"\x1b[B".to_vec(),
            Key::Named(NamedKey::ArrowRight) => b"\x1b[C".to_vec(),
            Key::Named(NamedKey::ArrowLeft) => b"\x1b[D".to_vec(),
            Key::Named(NamedKey::Home) => b"\x1b[H".to_vec(),
            Key::Named(NamedKey::End) => b"\x1b[F".to_vec(),
            Key::Named(NamedKey::Insert) => b"\x1b[2~".to_vec(),
            Key::Named(NamedKey::Delete) => b"\x1b[3~".to_vec(),
            Key::Named(NamedKey::PageUp) => b"\x1b[5~".to_vec(),
            Key::Named(NamedKey::PageDown) => b"\x1b[6~".to_vec(),
            _ => text.map(|text| text.as_bytes().to_vec())?,
        };
        if alt {
            base.insert(0, 0x1b);
        }
        Some(base)
    }

    fn control(c: &str) -> Option<Vec<u8>> {
        let held = c.as_bytes();
        if held.len() != 1 {
            return None;
        }
        let code = match held[0] {
            b'a'..=b'z' => held[0] - b'a' + 1,
            b'A'..=b'Z' => held[0] - b'A' + 1,
            b' ' | b'@' => 0,
            b'[' => 27,
            b'\\' => 28,
            b']' => 29,
            b'^' => 30,
            b'_' | b'/' => 31,
            b'?' => 127,
            _ => return None,
        };
        Some(vec![code])
    }

    fn modified(key: &NamedKey) -> Option<Vec<u8>> {
        let seq = match key {
            NamedKey::ArrowUp => "\x1b[1;5A",
            NamedKey::ArrowDown => "\x1b[1;5B",
            NamedKey::ArrowRight => "\x1b[1;5C",
            NamedKey::ArrowLeft => "\x1b[1;5D",
            NamedKey::Home => "\x1b[1;5H",
            NamedKey::End => "\x1b[1;5F",
            NamedKey::Insert => "\x1b[2;5~",
            NamedKey::Delete => "\x1b[3;5~",
            NamedKey::PageUp => "\x1b[5;5~",
            NamedKey::PageDown => "\x1b[6;5~",
            _ => return None,
        };
        Some(seq.as_bytes().to_vec())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn enter_sends_cr() {
        let key = Shell::key(
            ElementState::Pressed,
            &Key::Named(NamedKey::Enter),
            Some("\r"),
            false,
            false,
        );
        assert_eq!(key, Some(b"\r".to_vec()));
    }

    #[test]
    fn release_sends_nothing() {
        let key = Shell::key(
            ElementState::Released,
            &Key::Named(NamedKey::Enter),
            Some("\r"),
            false,
            false,
        );
        assert_eq!(key, None);
    }

    #[test]
    fn arrows_send_csi() {
        let up = Shell::key(
            ElementState::Pressed,
            &Key::Named(NamedKey::ArrowUp),
            None,
            false,
            false,
        );
        assert_eq!(up, Some(b"\x1b[A".to_vec()));
        let down = Shell::key(
            ElementState::Pressed,
            &Key::Named(NamedKey::ArrowDown),
            None,
            false,
            false,
        );
        assert_eq!(down, Some(b"\x1b[B".to_vec()));
        let del = Shell::key(
            ElementState::Pressed,
            &Key::Named(NamedKey::Delete),
            None,
            false,
            false,
        );
        assert_eq!(del, Some(b"\x1b[3~".to_vec()));
    }

    #[test]
    fn text_sends_bytes() {
        let key = Shell::key(
            ElementState::Pressed,
            &Key::Character("l".into()),
            Some("l"),
            false,
            false,
        );
        assert_eq!(key, Some(b"l".to_vec()));
    }

    #[test]
    fn ctrl_sends_codes() {
        let rom = |logical: &Key| Shell::key(ElementState::Pressed, logical, None, true, false);
        assert_eq!(
            rom(&Key::Character("c".into())),
            Some(vec![3]),
            "ctrl+c interrupts"
        );
        assert_eq!(rom(&Key::Character("d".into())), Some(vec![4]), "ctrl+d exits");
        assert_eq!(rom(&Key::Character("C".into())), Some(vec![3]), "caps maps same");
        assert_eq!(
            rom(&Key::Named(NamedKey::ArrowUp)),
            Some(b"\x1b[1;5A".to_vec()),
            "ctrl+arrows modify"
        );
        assert_eq!(
            rom(&Key::Named(NamedKey::Enter)),
            None,
            "ctrl+enter has no code"
        );
    }

    #[test]
    fn alt_prefixes_esc() {
        let key = Shell::key(
            ElementState::Pressed,
            &Key::Character("f".into()),
            Some("f"),
            false,
            true,
        );
        assert_eq!(key, Some(b"\x1bf".to_vec()));
    }
}
