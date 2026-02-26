use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: i64,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct LogStreamState {
    pub log_group: String,
    pub log_stream: Option<String>,
    pub entries: Vec<LogEntry>,
    pub next_forward_token: Option<String>,
}

impl LogStreamState {
    pub fn new(log_group: String) -> Self {
        Self {
            log_group,
            log_stream: None,
            entries: Vec::new(),
            next_forward_token: None,
        }
    }
}

#[derive(Debug)]
pub struct LogViewerState {
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    pub per_step_cache: HashMap<String, LogStreamState>,
}

impl LogViewerState {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            auto_scroll: true,
            per_step_cache: HashMap::new(),
        }
    }

    pub fn entries_for_step(&self, step_name: &str) -> &[LogEntry] {
        self.per_step_cache
            .get(step_name)
            .map(|s| s.entries.as_slice())
            .unwrap_or(&[])
    }

    pub fn scroll_down(&mut self, step_name: &str, amount: usize) {
        let total = self.entries_for_step(step_name).len();
        if total == 0 {
            return;
        }
        self.scroll_offset = self.scroll_offset.saturating_add(amount).min(total - 1);
        self.auto_scroll = false;
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
        self.auto_scroll = false;
    }

    pub fn jump_to_end(&mut self, step_name: &str) {
        let total = self.entries_for_step(step_name).len();
        self.scroll_offset = total.saturating_sub(1);
        self.auto_scroll = true;
    }

    pub fn jump_to_start(&mut self) {
        self.scroll_offset = 0;
        self.auto_scroll = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewer_with_entries(step: &str, count: usize) -> LogViewerState {
        let mut viewer = LogViewerState::new();
        let entries: Vec<LogEntry> = (0..count)
            .map(|i| LogEntry {
                timestamp: i as i64,
                message: format!("line {}", i),
            })
            .collect();
        viewer
            .per_step_cache
            .insert(step.to_string(), LogStreamState {
                log_group: "test".to_string(),
                log_stream: None,
                entries,
                next_forward_token: None,
            });
        viewer
    }

    #[test]
    fn entries_for_missing_step_returns_empty() {
        let viewer = LogViewerState::new();
        assert!(viewer.entries_for_step("nonexistent").is_empty());
    }

    #[test]
    fn entries_for_step_returns_correct_slice() {
        let viewer = viewer_with_entries("step1", 5);
        assert_eq!(viewer.entries_for_step("step1").len(), 5);
    }

    #[test]
    fn scroll_down_increments_and_disables_auto() {
        let mut viewer = viewer_with_entries("s", 10);
        viewer.scroll_down("s", 3);
        assert_eq!(viewer.scroll_offset, 3);
        assert!(!viewer.auto_scroll);
    }

    #[test]
    fn scroll_down_caps_at_max() {
        let mut viewer = viewer_with_entries("s", 5);
        viewer.scroll_down("s", 100);
        assert_eq!(viewer.scroll_offset, 4); // 5 - 1
    }

    #[test]
    fn scroll_up_saturates_at_zero() {
        let mut viewer = viewer_with_entries("s", 10);
        viewer.scroll_offset = 2;
        viewer.scroll_up(5);
        assert_eq!(viewer.scroll_offset, 0);
        assert!(!viewer.auto_scroll);
    }

    #[test]
    fn jump_to_end_enables_auto_scroll() {
        let mut viewer = viewer_with_entries("s", 10);
        viewer.auto_scroll = false;
        viewer.jump_to_end("s");
        assert_eq!(viewer.scroll_offset, 9);
        assert!(viewer.auto_scroll);
    }

    #[test]
    fn jump_to_start_disables_auto_scroll() {
        let mut viewer = viewer_with_entries("s", 10);
        viewer.scroll_offset = 5;
        viewer.jump_to_start();
        assert_eq!(viewer.scroll_offset, 0);
        assert!(!viewer.auto_scroll);
    }

    #[test]
    fn scroll_down_on_empty_step() {
        let mut viewer = LogViewerState::new();
        viewer.scroll_down("empty", 5);
        assert_eq!(viewer.scroll_offset, 0);
    }
}
