use std::io::Write;
use std::process::{Command, Stdio};

pub struct Clip;

impl Clip {
    pub fn copy(text: &str) {
        for args in [&[][..], &["--primary"][..]] {
            if let Ok(mut child) = Command::new("wl-copy")
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

    pub fn paste(primary: bool) -> String {        let mut cmd = Command::new("wl-paste");
        if primary {
            cmd.arg("--primary");
        }
        cmd.output()
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
            .unwrap_or_default()
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
