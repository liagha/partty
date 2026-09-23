use std::io::Write;
use std::process::{Command, Stdio};

pub struct Clip;

impl Clip {
    pub fn copy(text: &str) {
        Self::put(text);
    }

    pub fn paste(primary: bool) -> String {
        Self::get(primary)
    }

    pub fn wrap(text: &str, bracket: bool) -> Vec<u8> {
        if bracket {
            let mut out = Vec::with_capacity(text.len() + 12);
            out.extend_from_slice(b"\x1b[2004~");
            out.extend_from_slice(text.as_bytes());
            out.extend_from_slice(b"\x1b[201~");
            out
        } else {
            text.as_bytes().to_vec()
        }
    }

    fn pipe(prog: &str, args: &[&str], text: &str) {
        if let Ok(mut child) = Command::new(prog)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
        }
    }

    fn read(prog: &str, args: &[&str]) -> String {
        Command::new(prog)
            .args(args)
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
            .unwrap_or_default()
    }

    #[cfg(target_os = "linux")]
    fn put(text: &str) {
        Self::pipe("wl-copy", &[], text);
        Self::pipe("wl-copy", &["--primary"], text);
    }

    #[cfg(target_os = "macos")]
    fn put(text: &str) {
        Self::pipe("pbcopy", &[], text);
    }

    #[cfg(target_os = "windows")]
    fn put(text: &str) {
        Self::pipe("clip", &[], text);
    }

    #[cfg(all(unix, not(target_os = "linux"), not(target_os = "macos")))]
    fn put(text: &str) {
        Self::pipe("xclip", &["-selection", "clipboard"], text);
        Self::pipe("xclip", &["-selection", "primary"], text);
    }

    #[cfg(not(any(unix, target_os = "windows")))]
    fn put(_text: &str) {}

    #[cfg(target_os = "linux")]
    fn get(primary: bool) -> String {
        if primary {
            Self::read("wl-paste", &["--primary"])
        } else {
            Self::read("wl-paste", &[])
        }
    }

    #[cfg(target_os = "macos")]
    fn get(_primary: bool) -> String {
        Self::read("pbpaste", &[])
    }

    #[cfg(target_os = "windows")]
    fn get(_primary: bool) -> String {
        Self::read(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "$OutputEncoding = [Console]::OutputEncoding = [Text.UTF8Encoding]::UTF8; Get-Clipboard -Raw",
            ],
        )
    }

    #[cfg(all(unix, not(target_os = "linux"), not(target_os = "macos")))]
    fn get(primary: bool) -> String {
        if primary {
            Self::read("xclip", &["-o", "-selection", "primary"])
        } else {
            Self::read("xclip", &["-o", "-selection", "clipboard"])
        }
    }

    #[cfg(not(any(unix, target_os = "windows")))]
    fn get(_primary: bool) -> String {
        String::new()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn wrap_brackets() {
        assert_eq!(Clip::wrap("hi", true), b"\x1b[2004~hi\x1b[201~");
        assert_eq!(Clip::wrap("hi", false), b"hi");
    }
}
