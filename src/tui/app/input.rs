/// Stdin reader — blocking thread that parses raw terminal input into Events.
use crate::event::{Event, KeyEvent};
use crate::tui::term;
use std::time::Duration;
use tokio::sync::mpsc;

const ESC_TIMEOUT: Duration = Duration::from_millis(50);

/// Read stdin in a blocking loop, sending parsed Events.
pub fn read_stdin_loop(tx: mpsc::Sender<Event>) {
    use std::io::Read;
    let stdin = std::io::stdin();
    let mut raw = [0u8; 4096];
    let mut pending = Vec::<u8>::new();

    loop {
        // Drain all complete events from pending buffer
        loop {
            if pending.is_empty() { break; }
            match try_parse_event(&pending) {
                ParseResult::Event(event, consumed) => {
                    // Never block: drop events if channel full rather than
                    // freezing the input thread (and the entire UI).
                    match tx.try_send(event) {
                        Ok(()) => {}
                        Err(mpsc::error::TrySendError::Full(_)) => {
                            // Channel full — UI is busy. Drop this event.
                            // User will retry the keypress.
                        }
                        Err(mpsc::error::TrySendError::Closed(_)) => return,
                    }
                    pending.drain(..consumed);
                }
                ParseResult::Incomplete => break,
                ParseResult::Unknown => { pending.remove(0); }
            }
        }

        // Bare ESC disambiguation with short timeout
        if !pending.is_empty() && pending[0] == 0x1b {
            if poll_stdin(ESC_TIMEOUT) {
                let n = match stdin.lock().read(&mut raw) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                pending.extend_from_slice(&raw[..n]);
            } else {
                if let Err(mpsc::error::TrySendError::Closed(_)) =
                    tx.try_send(Event::Key(KeyEvent::Escape))
                {
                    return;
                }
                pending.remove(0);
            }
            continue;
        }

        // Normal blocking read
        let n = match stdin.lock().read(&mut raw) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        pending.extend_from_slice(&raw[..n]);
    }
}

/// Poll stdin for readability with timeout.
fn poll_stdin(timeout: Duration) -> bool {
    use std::os::fd::AsRawFd;
    let fd = std::io::stdin().as_raw_fd();
    let mut pollfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let ms = timeout.as_millis() as i32;
    // Safety: libc::poll on a valid fd with correct nfds
    let ret = unsafe { libc::poll(&mut pollfd, 1, ms) };
    ret > 0 && (pollfd.revents & libc::POLLIN) != 0
}

enum ParseResult {
    Event(Event, usize),
    Incomplete,
    Unknown,
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn try_parse_event(buf: &[u8]) -> ParseResult {
    if buf.is_empty() { return ParseResult::Incomplete; }

    // Bracketed paste: ESC[200~ ... ESC[201~
    const PASTE_START: &[u8] = b"\x1b[200~";
    const PASTE_END: &[u8] = b"\x1b[201~";
    if buf.starts_with(PASTE_START) {
        if let Some(end_pos) = find_subsequence(buf, PASTE_END) {
            let content = &buf[PASTE_START.len()..end_pos];
            let text = String::from_utf8_lossy(content).into_owned();
            let consumed = end_pos + PASTE_END.len();
            return ParseResult::Event(Event::Key(KeyEvent::Paste(text)), consumed);
        }
        return ParseResult::Incomplete;
    }

    // SGR mouse: ESC [ < digits ; digits ; digits M/m
    if buf.starts_with(b"\x1b[<") {
        if let Some(end) = buf.iter().position(|&b| b == b'M' || b == b'm') {
            if let Some(input) = term::parse_input(&buf[..=end]) {
                let event = match input {
                    term::InputEvent::Key(k) => Event::Key(k),
                    term::InputEvent::Mouse(m) => Event::Mouse(m),
                };
                return ParseResult::Event(event, end + 1);
            }
            return ParseResult::Unknown;
        }
        if buf.len() < 20 { return ParseResult::Incomplete; }
        return ParseResult::Unknown;
    }

    // Arrow keys / other ESC sequences: ESC [ A/B/C/D
    if buf.starts_with(b"\x1b[") && buf.len() >= 3 {
        if let Some(input) = term::parse_input(&buf[..3]) {
            let event = match input {
                term::InputEvent::Key(k) => Event::Key(k),
                term::InputEvent::Mouse(m) => Event::Mouse(m),
            };
            return ParseResult::Event(event, 3);
        }
        return ParseResult::Unknown;
    }

    // ESC prefix handling
    if buf[0] == 0x1b {
        if buf.len() == 1 { return ParseResult::Incomplete; }
        if buf[1] == b'\r' {
            return ParseResult::Event(Event::Key(KeyEvent::AltEnter), 2);
        }
        if buf[1] != b'[' {
            return ParseResult::Event(Event::Key(KeyEvent::Escape), 1);
        }
        if buf.len() == 2 { return ParseResult::Incomplete; }
    }

    // Multi-byte UTF-8 char
    let ch_len = utf8_char_len(buf[0]);
    if ch_len > 1 {
        if buf.len() < ch_len { return ParseResult::Incomplete; }
        if let Ok(s) = std::str::from_utf8(&buf[..ch_len])
            && let Some(c) = s.chars().next()
        {
            return ParseResult::Event(Event::Key(KeyEvent::Char(c)), ch_len);
        }
        return ParseResult::Unknown;
    }

    // Single ASCII byte
    if let Some(input) = term::parse_input(&buf[..1]) {
        let event = match input {
            term::InputEvent::Key(k) => Event::Key(k),
            term::InputEvent::Mouse(m) => Event::Mouse(m),
        };
        return ParseResult::Event(event, 1);
    }

    // Fallback single-byte UTF-8
    if let Ok(s) = std::str::from_utf8(&buf[..ch_len])
        && let Some(c) = s.chars().next()
    {
        return ParseResult::Event(Event::Key(KeyEvent::Char(c)), ch_len);
    }

    ParseResult::Unknown
}

fn utf8_char_len(first_byte: u8) -> usize {
    match first_byte {
        0..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xFF => 4,
        _ => 1,
    }
}
