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
        self.scroll_offset = self.scroll_offset.saturating_add(amount).min(total.saturating_sub(1));
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
