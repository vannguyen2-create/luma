/// Low-level terminal: raw mode, ANSI escapes, mouse parsing, clipboard.
use crate::event::{KeyEvent, MouseButton, MouseEvent};
use std::io::{self, BufWriter, Stdout, Write};

/// ANSI escape sequences.
pub const ALT_ON: &str = "\x1b[?1049h";
pub const ALT_OFF: &str = "\x1b[?1049l";
pub const CURSOR_HIDE: &str = "\x1b[?25l";
pub const CURSOR_SHOW: &str = "\x1b[?25h";
pub const MOUSE_ON: &str = "\x1b[?1002h\x1b[?1006h";
pub const MOUSE_OFF: &str = "\x1b[?1002l\x1b[?1006l";
pub const PASTE_ON: &str = "\x1b[?2004h";
pub const PASTE_OFF: &str = "\x1b[?2004l";
pub const RESET: &str = "\x1b[0m";

/// Copy text to system clipboard via OSC 52.
pub fn copy_to_clipboard(out: &mut impl Write, text: &str) -> io::Result<()> {
    let b64 = base64_encode(text.as_bytes());
    write!(out, "\x1b]52;c;{b64}\x1b\\")
}

/// Create a buffered stdout writer.
pub fn buffered_stdout() -> BufWriter<Stdout> {
    BufWriter::with_capacity(8192, io::stdout())
}

/// Enter raw mode. Returns the previous termios to restore later.
#[cfg(unix)]
pub fn enter_raw() -> io::Result<libc::termios> {
    use std::mem::MaybeUninit;
    unsafe {
        let mut orig = MaybeUninit::uninit();
        if libc::tcgetattr(0, orig.as_mut_ptr()) != 0 {
            return Err(io::Error::last_os_error());
        }
        let orig = orig.assume_init();
        let mut raw = orig;
        libc::cfmakeraw(&mut raw);
        if libc::tcsetattr(0, libc::TCSANOW, &raw) != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(orig)
    }
}

/// Restore terminal from raw mode.
#[cfg(unix)]
pub fn exit_raw(orig: &libc::termios) {
    unsafe {
        libc::tcsetattr(0, libc::TCSANOW, orig);
    }
}

/// Terminal dimensions.
pub fn size() -> (u16, u16) {
    #[cfg(unix)]
    {
        use std::mem::MaybeUninit;
        unsafe {
            let mut ws = MaybeUninit::<libc::winsize>::uninit();
            if libc::ioctl(1, libc::TIOCGWINSZ, ws.as_mut_ptr()) == 0 {
                let ws = ws.assume_init();
                return (ws.ws_col, ws.ws_row);
            }
        }
    }
    (80, 24)
}

/// Parse a single input byte sequence into a KeyEvent or MouseEvent.
pub fn parse_input(data: &[u8]) -> Option<InputEvent> {
    if data.is_empty() {
        return None;
    }

    // Try mouse first (SGR format: ESC [ < ... M/m)
    if let Some(mouse) = parse_mouse(data) {
        return Some(InputEvent::Mouse(mouse));
    }

    // Key sequences
    match data {
        b"\r" | b"\n" => Some(InputEvent::Key(KeyEvent::Enter)),
        b"\t" => Some(InputEvent::Key(KeyEvent::Tab)),
        b"\x7f" | b"\x08" => Some(InputEvent::Key(KeyEvent::Backspace)),
        b"\x1b" => Some(InputEvent::Key(KeyEvent::Escape)),
        b"\x03" => Some(InputEvent::Key(KeyEvent::CtrlC)),
        b"\x14" => Some(InputEvent::Key(KeyEvent::CtrlT)),
        b"\x01" => Some(InputEvent::Key(KeyEvent::CtrlA)),
        b"\x05" => Some(InputEvent::Key(KeyEvent::CtrlE)),
        b"\x15" => Some(InputEvent::Key(KeyEvent::CtrlU)),
        b"\x1b[A" => Some(InputEvent::Key(KeyEvent::ArrowUp)),
        b"\x1b[B" => Some(InputEvent::Key(KeyEvent::ArrowDown)),
        b"\x1b[D" => Some(InputEvent::Key(KeyEvent::ArrowLeft)),
        b"\x1b[C" => Some(InputEvent::Key(KeyEvent::ArrowRight)),
        [c] if *c >= b' ' => Some(InputEvent::Key(KeyEvent::Char(*c as char))),
        _ => None,
    }
}

/// Parsed input — either a key or mouse event.
pub enum InputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
}

fn parse_mouse(data: &[u8]) -> Option<MouseEvent> {
    // SGR format: ESC [ < code ; col ; row M/m
    let s = std::str::from_utf8(data).ok()?;
    if !s.starts_with("\x1b[<") {
        return None;
    }
    let suffix = s.strip_prefix("\x1b[<")?;

    let is_release = suffix.ends_with('m');
    let body = suffix.strip_suffix(if is_release { 'm' } else { 'M' })?;

    let mut parts = body.split(';');
    let code: u16 = parts.next()?.parse().ok()?;
    let col: u16 = parts.next()?.parse().ok()?;
    let row: u16 = parts.next()?.parse().ok()?;

    if code == 64 {
        return Some(MouseEvent::ScrollUp { row, col });
    }
    if code == 65 {
        return Some(MouseEvent::ScrollDown { row, col });
    }

    let button = match code & 3 {
        0 => MouseButton::Left,
        1 => MouseButton::Middle,
        2 => MouseButton::Right,
        _ => MouseButton::None,
    };

    if is_release {
        return Some(MouseEvent::Release { button, row, col });
    }
    if code & 32 != 0 {
        return Some(MouseEvent::Drag { button, row, col });
    }
    Some(MouseEvent::Press { button, row, col })
}

/// Minimal base64 encode (no padding needed for OSC 52).
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[((n >> 18) & 63) as usize] as char);
        out.push(CHARS[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARS[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARS[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scroll_up() {
        let data = b"\x1b[<64;10;5M";
        let evt = parse_mouse(data).unwrap();
        assert!(matches!(evt, MouseEvent::ScrollUp { row: 5, col: 10 }));
    }

    #[test]
    fn parse_left_press() {
        let data = b"\x1b[<0;20;10M";
        let evt = parse_mouse(data).unwrap();
        assert!(matches!(
            evt,
            MouseEvent::Press {
                button: MouseButton::Left,
                row: 10,
                col: 20
            }
        ));
    }

    #[test]
    fn parse_left_release() {
        let data = b"\x1b[<0;20;10m";
        let evt = parse_mouse(data).unwrap();
        assert!(matches!(
            evt,
            MouseEvent::Release {
                button: MouseButton::Left,
                row: 10,
                col: 20
            }
        ));
    }

    #[test]
    fn parse_drag() {
        let data = b"\x1b[<32;25;12M";
        let evt = parse_mouse(data).unwrap();
        assert!(matches!(
            evt,
            MouseEvent::Drag {
                button: MouseButton::Left,
                row: 12,
                col: 25
            }
        ));
    }

    #[test]
    fn parse_key_enter() {
        let evt = parse_input(b"\r").unwrap();
        assert!(matches!(evt, InputEvent::Key(KeyEvent::Enter)));
    }

    #[test]
    fn parse_key_char() {
        let evt = parse_input(b"a").unwrap();
        assert!(matches!(evt, InputEvent::Key(KeyEvent::Char('a'))));
    }

    #[test]
    fn parse_arrow_up() {
        let evt = parse_input(b"\x1b[A").unwrap();
        assert!(matches!(evt, InputEvent::Key(KeyEvent::ArrowUp)));
    }

    #[test]
    fn base64_encode_hello() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
    }

    #[test]
    fn base64_encode_empty() {
        assert_eq!(base64_encode(b""), "");
    }
}
