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

    pub fn paste(primary: bool) -> String {
        let mut cmd = Command::new("wl-paste");
        if primary {
            cmd.arg("--primary");
        }
        cmd.output()
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
            .unwrap_or_default()
    }
}
