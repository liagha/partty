use std::fs::OpenOptions;
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
        let _ = std::fs::write("/tmp/partty-pty.log", "");
        let system = portable_pty::native_pty_system();
        let pair = system.openpty(Self::size(rows, cols)).unwrap();
        let name = std::env::var("SHELL").unwrap_or_else(|_| "bash".into());
        let mut cmd = portable_pty::CommandBuilder::new(name);
        cmd.env("TERM", "xterm-256color");
        let child = pair.slave.spawn_command(cmd).unwrap();
        let mut reader = pair.master.try_clone_reader().unwrap();
        let writer = pair.master.take_writer().unwrap();
        let (send, recv) = channel();
        thread::spawn(move || {
            let mut log = OpenOptions::new()
                .append(true)
                .open("/tmp/partty-pty.log")
                .unwrap();
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let _ = log.write_all(&buf[..n]);
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

    pub fn resize(&self, rows: usize, cols: usize) {
        let _ = self._pair.master.resize(Self::size(rows, cols));
    }

    pub fn key(state: ElementState, repeat: bool, logical: &Key, text: Option<&str>) -> Option<Vec<u8>> {
        if state != ElementState::Pressed || repeat {
            return None;
        }
        match logical {
            Key::Named(NamedKey::Enter) => Some(b"\r".to_vec()),
            Key::Named(NamedKey::Backspace) => Some(b"\x7f".to_vec()),
            Key::Named(NamedKey::Tab) => Some(b"\t".to_vec()),
            Key::Named(NamedKey::Escape) => Some(b"\x1b".to_vec()),
            _ => text.map(|text| text.as_bytes().to_vec()),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn enter_sends_cr() {
        let key = Shell::key(
            ElementState::Pressed,
            false,
            &Key::Named(NamedKey::Enter),
            Some("\r"),
        );
        assert_eq!(key, Some(b"\r".to_vec()));
    }

    #[test]
    fn release_sends_nothing() {
        let key = Shell::key(
            ElementState::Released,
            false,
            &Key::Named(NamedKey::Enter),
            Some("\r"),
        );
        assert_eq!(key, None);
    }

    #[test]
    fn text_sends_bytes() {
        let key = Shell::key(
            ElementState::Pressed,
            false,
            &Key::Character("l".into()),
            Some("l"),
        );
        assert_eq!(key, Some(b"l".to_vec()));
    }
}
