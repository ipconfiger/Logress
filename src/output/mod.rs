pub mod formatter;
pub mod color;
pub mod screen;

use crate::cli::OutputFormat;
use crate::stream::parser::LogEntry;
use std::sync::Arc;

/// Central output handler: receives parsed log entries and writes them to stdout.
pub struct OutputHandler {
    format: OutputFormat,
    use_utc: bool,
    /// Shared screen state for freeze/pause interaction
    pub screen_state: Arc<screen::ScreenState>,
}

impl OutputHandler {
    pub fn new(format: OutputFormat, use_utc: bool, screen_state: Arc<screen::ScreenState>) -> Self {
        OutputHandler {
            format,
            use_utc,
            screen_state,
        }
    }

    /// Write a batch of log entries to stdout.
    /// If the screen is frozen, entries are silently dropped.
    pub fn write_entries(&self, entries: &[LogEntry]) {
        if self.screen_state.is_frozen() {
            return;
        }

        for entry in entries {
            let line = formatter::format_entry(entry, self.format, self.use_utc);
            println!("{line}");
        }
    }
}
