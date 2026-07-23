//! Frozen-screen interaction: press 'h' to pause/resume log display.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Represents the frozen/paused state of the log display
pub struct ScreenState {
    frozen: AtomicBool,
}

impl ScreenState {
    pub fn new() -> Self {
        ScreenState {
            frozen: AtomicBool::new(false),
        }
    }

    /// Check if the screen is currently frozen
    #[allow(dead_code)]
    pub fn is_frozen(&self) -> bool {
        self.frozen.load(Ordering::SeqCst)
    }

    /// Toggle the frozen state, returning the new state (true = frozen)
    pub fn toggle(&self) -> bool {
        let was = self.frozen.fetch_xor(true, Ordering::SeqCst);
        let now = !was;

        if now {
            eprint!("\r[PAUSED] Press h to resume...");
        } else {
            eprint!("\r                              \r");
        }

        now
    }
}

impl Default for ScreenState {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawn a keyboard listener that toggles the screen state on 'h' keypress.
///
/// Returns immediately; the listener runs in the background.
/// Requires `crossterm` raw mode to be enabled for `event::poll` to work correctly.
pub fn spawn_keyboard_listener(state: Arc<ScreenState>, cancel: tokio_util::sync::CancellationToken) {
    tokio::spawn(async move {
        use crossterm::event::{self, Event, KeyCode};

        loop {
            if cancel.is_cancelled() {
                break;
            }

            // Non-blocking poll: check every 100ms
            if event::poll(std::time::Duration::from_millis(100)).unwrap_or(false) {
                if let Ok(Event::Key(key_event)) = event::read() {
                    if key_event.code == KeyCode::Char('h') {
                        state.toggle();
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toggle_state() {
        let state = ScreenState::new();
        assert!(!state.is_frozen());

        let now = state.toggle();
        assert!(now);
        assert!(state.is_frozen());

        let now = state.toggle();
        assert!(!now);
        assert!(!state.is_frozen());
    }
}
