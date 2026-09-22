use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::sync::mpsc::{Receiver, channel};
use std::thread;

pub struct Shell {
    _child: Box<dyn portable_pty::Child + Send + Sync>,
    _pair: portable_pty::PtyPair,
}

impl Shell {
    pub fn open() -> (Self, Receiver<Vec<u8>>) {
        let _ = std::fs::write("/tmp/partty-pty.log", "");
        let system = portable_pty::native_pty_system();
        let pair = system
            .openpty(portable_pty::PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let name = std::env::var("SHELL").unwrap_or_else(|_| "bash".into());
        let mut cmd = portable_pty::CommandBuilder::new(name);
        cmd.env("TERM", "xterm-256color");
        let child = pair.slave.spawn_command(cmd).unwrap();
        let mut reader = pair.master.try_clone_reader().unwrap();
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
                    }
                }
            }
        });
        (
            Self {
                _child: child,
                _pair: pair,
            },
            recv,
        )
    }
}
