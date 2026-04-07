/// Stdin reader — blocking thread that reads crossterm events.
use crate::event::Event;
use crossterm::event::{self as ct, KeyEventKind};
use tokio::sync::mpsc;

/// Read terminal events in a blocking loop, sending parsed Events.
pub fn read_stdin_loop(tx: mpsc::Sender<Event>) {
    loop {
        let Ok(raw) = ct::read() else { break };

        // Only forward key-press events (ignore Release/Repeat).
        if let ct::Event::Key(ref k) = raw
            && k.kind != KeyEventKind::Press
        {
            continue;
        }

        match tx.try_send(Event::Term(raw)) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                // Channel full — UI is busy. Drop this event.
            }
            Err(mpsc::error::TrySendError::Closed(_)) => return,
        }
    }
}
