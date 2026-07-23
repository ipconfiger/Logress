//! Terminal color scheme: log level highlighting and label color assignment.

use owo_colors::Style;
use std::collections::HashMap;

/// Log level detection and coloring
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
    Unknown,
}

impl LogLevel {
    /// Detect log level from a log line
    pub fn detect(line: &str) -> Self {
        let lower = line.to_lowercase();

        // Check for structured log patterns first
        // e.g. level=ERROR, "ERROR", [ERROR], ERROR:
        let patterns = [
            ("trace", LogLevel::Trace),
            ("debug", LogLevel::Debug),
            ("fatal", LogLevel::Fatal),
            ("critical", LogLevel::Fatal),
            ("panic", LogLevel::Fatal),
            ("error", LogLevel::Error),
            ("err", LogLevel::Error),
            ("warn", LogLevel::Warn),
            ("warning", LogLevel::Warn),
            ("info", LogLevel::Info),
            ("notice", LogLevel::Info),
        ];

        for (keyword, level) in &patterns {
            // Match common patterns: level=KEYWORD, "KEYWORD", [KEYWORD], KEYWORD:
            if lower.contains(&format!("level={keyword}"))
                || lower.contains(&format!("\"{keyword}\""))
                || lower.contains(&format!("[{keyword}]"))
                || lower.contains(&format!("{keyword}:"))
                || lower.starts_with(keyword)
                || lower.contains(&format!(" {keyword} "))
                || lower.ends_with(&format!(" {keyword}"))
            {
                return *level;
            }
        }

        LogLevel::Unknown
    }

    /// Get the display color for this level
    pub fn style(&self) -> Style {
        match self {
            LogLevel::Trace => Style::new().bright_black(),
            LogLevel::Debug => Style::new().cyan(),
            LogLevel::Info => Style::new().green(),
            LogLevel::Warn => Style::new().yellow(),
            LogLevel::Error => Style::new().red(),
            LogLevel::Fatal => Style::new().white().on_red(),
            LogLevel::Unknown => Style::new(),
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Fatal => "FATAL",
            LogLevel::Unknown => "UNKNOWN",
        }
    }
}

/// Detect log level from a line (re-export for convenience)
pub fn detect_log_level(line: &str) -> LogLevel {
    LogLevel::detect(line)
}

/// Apply log-level-appropriate styling to a log line
pub fn style_log_line(line: &str, level: LogLevel) -> String {
    level.style().style(line).to_string()
}

// ─── Label color assignment ───────────────────────────────────────────────

/// A pool of 8 distinct colors for label assignment
const LABEL_COLORS: [fn(Style) -> Style; 8] = [
    |s| s.bright_blue(),
    |s| s.magenta(),
    |s| s.cyan(),
    |s| s.bright_green(),
    |s| s.bright_yellow(),
    |s| s.bright_red(),
    |s| s.bright_white(),
    |s| s.bright_black(),
];

/// Maps label names to assigned colors
pub struct LabelColorMap {
    mapping: HashMap<String, usize>,
    next_color: usize,
}

impl LabelColorMap {
    pub fn new() -> Self {
        LabelColorMap {
            mapping: HashMap::new(),
            next_color: 0,
        }
    }

    /// Get or assign a color index for a label key
    pub fn get_or_assign(&mut self, label_key: &str) -> usize {
        if let Some(&index) = self.mapping.get(label_key) {
            return index;
        }

        let index = self.next_color % LABEL_COLORS.len();
        self.mapping.insert(label_key.to_string(), index);
        self.next_color += 1;
        index
    }

    /// Apply color to a label key-value text
    pub fn colorize(&mut self, label_key: &str, value: &str) -> String {
        let color_index = self.get_or_assign(label_key);
        let style_fn = LABEL_COLORS[color_index];
        let text = format!("[{label_key}:{value}]");
        style_fn(Style::new()).style(text).to_string()
    }
}

/// Convenience function: colorize all labels in a map, skipping noise labels.
pub fn colorize_labels(labels: &HashMap<String, String>) -> String {
    // Labels to skip (noise / internal metadata)
    const SKIP_LABELS: &[&str] = &["filename", "detected_level"];

    let mut color_map = LabelColorMap::new();
    let parts: Vec<String> = labels
        .iter()
        .filter(|(k, _)| !SKIP_LABELS.contains(&k.as_str()))
        .map(|(k, v)| color_map.colorize(k, v))
        .collect();
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_log_level_error() {
        assert_eq!(LogLevel::detect("ERROR: something broke"), LogLevel::Error);
        assert_eq!(LogLevel::detect("level=error msg=oh no"), LogLevel::Error);
        assert_eq!(LogLevel::detect("this is an err in the log"), LogLevel::Error);
    }

    #[test]
    fn test_detect_log_level_warn() {
        assert_eq!(LogLevel::detect("WARNING: disk 90% full"), LogLevel::Warn);
        assert_eq!(LogLevel::detect("level=warn"), LogLevel::Warn);
    }

    #[test]
    fn test_detect_log_level_info() {
        assert_eq!(LogLevel::detect("INFO: server started"), LogLevel::Info);
        assert_eq!(LogLevel::detect("notice: please update"), LogLevel::Info);
    }

    #[test]
    fn test_detect_log_level_unknown() {
        assert_eq!(LogLevel::detect("192.168.1.1 - GET /"), LogLevel::Unknown);
    }

    #[test]
    fn test_label_color_map() {
        let mut map = LabelColorMap::new();
        let idx1 = map.get_or_assign("app");
        let idx2 = map.get_or_assign("app");
        assert_eq!(idx1, idx2); // Same key returns same color

        let idx3 = map.get_or_assign("pod");
        assert_ne!(idx1, idx3); // Different keys get different colors (pool of 8)
    }

    #[test]
    fn test_colorize_labels_output() {
        let mut labels = HashMap::new();
        labels.insert("app".into(), "nginx".into());
        let output = colorize_labels(&labels);
        assert!(output.contains("app"));
        assert!(output.contains("nginx"));
    }
}
