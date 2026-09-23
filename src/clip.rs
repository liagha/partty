#[cfg(target_os = "linux")]
use arboard::{GetExtLinux, LinuxClipboardKind};

pub struct Clip;

impl Clip {
    pub fn paste(primary: bool) -> String {
        let text = Self::get(primary);
        if text.is_empty() {
            Self::get(!primary)
        } else {
            text
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

    #[cfg(target_os = "linux")]
    fn get(primary: bool) -> String {
        if let Ok(mut clip) = arboard::Clipboard::new() {
            let kind = if primary {
                LinuxClipboardKind::Primary
            } else {
                LinuxClipboardKind::Clipboard
            };
            return clip.get().clipboard(kind).text().unwrap_or_default();
        }
        String::new()
    }

    #[cfg(not(target_os = "linux"))]
    fn get(_primary: bool) -> String {
        arboard::Clipboard::new()
            .and_then(|mut clip| clip.get_text())
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
