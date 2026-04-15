use chrono::{DateTime, Utc};

use crate::model::execution::{ExecutionSummary, PipelineExecution};
use crate::model::logs::LogViewerState;
use crate::model::metrics::MetricsState;
use crate::model::pipeline::{PipelineParameter, PipelineSummary};
use crate::model::step::{StepInfo, StepStatus};
use crate::notify;
use crate::polling::{MonitoringUpdate, PollError, PollResult};

/// Window after the last successful poll before we consider the view stale.
pub const STALE_THRESHOLD_SECS: i64 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleLevel {
    Fresh,
    Stale,
}

/// Decide whether the data in the UI is still fresh based on the last
/// successful poll time. `None` (no poll ever succeeded) is treated as stale.
pub fn stale_level(last_successful_poll: Option<DateTime<Utc>>, now: DateTime<Utc>) -> StaleLevel {
    match last_successful_poll {
        Some(t) if (now - t).num_seconds() <= STALE_THRESHOLD_SECS => StaleLevel::Fresh,
        _ => StaleLevel::Stale,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    SelectPipeline,
    SelectExecution,
    Monitoring,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorTab {
    Logs,
    Metrics,
}

#[derive(Debug, Clone)]
pub struct ParameterEditorState {
    pub pipeline_name: String,
    pub parameters: Vec<PipelineParameter>,
    pub values: Vec<String>,
    pub cursor: usize,
    pub loading: bool,
    pub error: Option<String>,
}

impl ParameterEditorState {
    /// Return `(name, value)` pairs to send to SageMaker on start. Required
    /// parameters are always included; optional parameters are only included
    /// when the user changed them from their definition default.
    pub fn overrides(&self) -> Vec<(String, String)> {
        self.parameters
            .iter()
            .zip(self.values.iter())
            .filter_map(|(p, v)| match &p.default_value {
                Some(default) if default == v => None,
                _ => Some((p.name.clone(), v.clone())),
            })
            .collect()
    }
}

pub struct App {
    pub mode: AppMode,
    pub pipelines: Vec<PipelineSummary>,
    pub pipeline_cursor: usize,
    pub selected_pipeline_name: Option<String>,
    pub executions: Vec<ExecutionSummary>,
    pub execution_cursor: usize,
    pub execution: Option<PipelineExecution>,
    pub steps: Vec<StepInfo>,
    pub selected_step: usize,
    pub auto_follow: bool,
    pub log_viewer: LogViewerState,
    pub active_tab: MonitorTab,
    pub metrics_state: MetricsState,
    pub should_quit: bool,
    pub error_message: Option<String>,
    pub loading: bool,
    pub notifications_enabled: bool,
    pub background_watcher_count: usize,
    /// Most recent poll error (credentials expired, transient failure, etc).
    /// Cleared on the next successful poll result.
    pub last_poll_error: Option<PollError>,
    /// Timestamp of the most recent successful poll (any variant).
    pub last_successful_poll: Option<DateTime<Utc>>,
    /// Active parameter editor overlay, if any.
    pub parameter_editor: Option<ParameterEditorState>,
}

impl App {
    pub fn new() -> Self {
        Self {
            mode: AppMode::SelectPipeline,
            pipelines: Vec::new(),
            pipeline_cursor: 0,
            selected_pipeline_name: None,
            executions: Vec::new(),
            execution_cursor: 0,
            execution: None,
            steps: Vec::new(),
            selected_step: 0,
            auto_follow: true,
            log_viewer: LogViewerState::new(),
            active_tab: MonitorTab::Logs,
            metrics_state: MetricsState::new(),
            should_quit: false,
            error_message: None,
            loading: true,
            notifications_enabled: false,
            background_watcher_count: 0,
            last_poll_error: None,
            last_successful_poll: None,
            parameter_editor: None,
        }
    }

    pub fn open_parameter_editor(&mut self, pipeline_name: String) {
        self.parameter_editor = Some(ParameterEditorState {
            pipeline_name,
            parameters: Vec::new(),
            values: Vec::new(),
            cursor: 0,
            loading: true,
            error: None,
        });
    }

    pub fn populate_parameter_editor(&mut self, parameters: Vec<PipelineParameter>) {
        if let Some(editor) = self.parameter_editor.as_mut() {
            editor.values = parameters.iter().map(|p| p.initial_value.clone()).collect();
            editor.parameters = parameters;
            editor.cursor = 0;
            editor.loading = false;
        }
    }

    pub fn close_parameter_editor(&mut self) {
        self.parameter_editor = None;
    }

    pub fn parameter_editor_cursor_up(&mut self) {
        if let Some(editor) = self.parameter_editor.as_mut() {
            if editor.cursor > 0 {
                editor.cursor -= 1;
            }
        }
    }

    pub fn parameter_editor_cursor_down(&mut self) {
        if let Some(editor) = self.parameter_editor.as_mut() {
            if editor.cursor + 1 < editor.values.len() {
                editor.cursor += 1;
            }
        }
    }

    pub fn parameter_editor_input(&mut self, c: char) {
        if let Some(editor) = self.parameter_editor.as_mut() {
            if let Some(value) = editor.values.get_mut(editor.cursor) {
                value.push(c);
            }
        }
    }

    pub fn parameter_editor_backspace(&mut self) {
        if let Some(editor) = self.parameter_editor.as_mut() {
            if let Some(value) = editor.values.get_mut(editor.cursor) {
                value.pop();
            }
        }
    }

    pub fn parameter_editor_clear_field(&mut self) {
        if let Some(editor) = self.parameter_editor.as_mut() {
            if let Some(value) = editor.values.get_mut(editor.cursor) {
                value.clear();
            }
        }
    }

    pub fn selected_step_name(&self) -> Option<&str> {
        self.steps.get(self.selected_step).map(|s| s.name.as_str())
    }

    pub fn select_step_up(&mut self) {
        if self.selected_step > 0 {
            self.selected_step -= 1;
            self.auto_follow = false;
            self.on_step_changed();
        }
    }

    pub fn select_step_down(&mut self) {
        if !self.steps.is_empty() && self.selected_step < self.steps.len().saturating_sub(1) {
            self.selected_step += 1;
            self.auto_follow = false;
            self.on_step_changed();
        }
    }

    pub fn execution_cursor_up(&mut self) {
        if self.execution_cursor > 0 {
            self.execution_cursor -= 1;
        }
    }

    pub fn execution_cursor_down(&mut self) {
        if self.execution_cursor < self.executions.len().saturating_sub(1) {
            self.execution_cursor += 1;
        }
    }

    pub fn selected_execution_arn(&self) -> Option<&str> {
        self.executions
            .get(self.execution_cursor)
            .map(|e| e.arn.as_str())
    }

    pub fn pipeline_cursor_up(&mut self) {
        if self.pipeline_cursor > 0 {
            self.pipeline_cursor -= 1;
        }
    }

    pub fn pipeline_cursor_down(&mut self) {
        if self.pipeline_cursor < self.pipelines.len().saturating_sub(1) {
            self.pipeline_cursor += 1;
        }
    }

    pub fn selected_pipeline_name(&self) -> Option<&str> {
        self.pipelines
            .get(self.pipeline_cursor)
            .map(|p| p.name.as_str())
    }

    /// Update steps from a poll result, preserving the current selection by step name.
    pub fn update_steps(&mut self, new_steps: Vec<StepInfo>) {
        let prev_name = self.selected_step_name().map(|s| s.to_string());

        self.steps = new_steps;

        // Try to restore selection by name
        if let Some(ref name) = prev_name {
            if let Some(pos) = self.steps.iter().position(|s| s.name == *name) {
                self.selected_step = pos;
                return;
            }
        }

        // Clamp selection to valid range
        if self.steps.is_empty() {
            self.selected_step = 0;
        } else if self.selected_step >= self.steps.len() {
            self.selected_step = self.steps.len() - 1;
        }
    }

    fn on_step_changed(&mut self) {
        // Reset scroll to end for the new step
        if let Some(name) = self.selected_step_name() {
            let name = name.to_string();
            self.log_viewer.jump_to_end(&name);
        }
        self.metrics_state.reset_selection();
    }

    /// Auto-follow: if enabled, move selection to the currently executing step
    pub fn maybe_follow_executing_step(&mut self) {
        if !self.auto_follow {
            return;
        }
        for (i, step) in self.steps.iter().enumerate() {
            if step.status == StepStatus::Executing {
                if self.selected_step != i {
                    self.selected_step = i;
                    self.on_step_changed();
                }
                return;
            }
        }
    }

    pub fn toggle_tab(&mut self) {
        self.active_tab = match self.active_tab {
            MonitorTab::Logs => MonitorTab::Metrics,
            MonitorTab::Metrics => MonitorTab::Logs,
        };
    }

    pub fn toggle_notifications(&mut self) {
        self.notifications_enabled = !self.notifications_enabled;
    }

    /// Dispatch a poll result from the background polling task.
    pub fn apply_poll_result(&mut self, result: PollResult) {
        match result {
            PollResult::Monitoring(update) => self.apply_monitoring_result(*update),
            PollResult::ExecutionList {
                pipeline_name,
                executions,
            } => self.apply_execution_list_result(&pipeline_name, executions),
            PollResult::Error(err) => self.apply_poll_error(err),
        }
    }

    /// Apply a monitoring update (steps + logs + metrics for a single execution).
    pub fn apply_monitoring_result(&mut self, result: MonitoringUpdate) {
        if self.notifications_enabled {
            let step_events = notify::detect_step_transitions(&self.steps, &result.steps);
            for event in &step_events {
                notify::send(event);
            }

            if let Some(ref old_exec) = self.execution {
                let pipeline_name = self.selected_pipeline_name.as_deref().unwrap_or("pipeline");
                if let Some(event) =
                    notify::detect_execution_transition(old_exec, &result.execution, pipeline_name)
                {
                    notify::send(&event);
                }
            }
        }

        self.execution = Some(result.execution);
        self.update_steps(result.steps);
        self.maybe_follow_executing_step();

        if let (Some(step_name), Some(stream_state)) =
            (result.log_step_name, result.log_stream_state)
        {
            self.log_viewer
                .per_step_cache
                .insert(step_name, stream_state);
        }

        if let (Some(step_name), Some(step_metrics)) = (result.metrics_step_name, result.metrics) {
            self.metrics_state
                .per_step_cache
                .insert(step_name, step_metrics);
        }

        self.mark_poll_success();
    }

    /// Apply a refreshed execution list (SelectExecution screen polling).
    /// Only replaces the list if it belongs to the currently-selected pipeline.
    pub fn apply_execution_list_result(
        &mut self,
        pipeline_name: &str,
        executions: Vec<ExecutionSummary>,
    ) {
        if self.selected_pipeline_name.as_deref() != Some(pipeline_name) {
            // User already navigated away — drop the stale update but still
            // count it as a healthy poll cycle.
            self.mark_poll_success();
            return;
        }

        self.executions = executions;
        if self.executions.is_empty() {
            self.execution_cursor = 0;
        } else if self.execution_cursor >= self.executions.len() {
            self.execution_cursor = self.executions.len() - 1;
        }

        self.mark_poll_success();
    }

    fn apply_poll_error(&mut self, err: PollError) {
        self.last_poll_error = Some(err);
    }

    fn mark_poll_success(&mut self) {
        self.last_poll_error = None;
        self.last_successful_poll = Some(Utc::now());
    }

    /// Transition into monitoring mode. Returns the initial step name for the poller.
    pub fn enter_monitoring(&mut self, _arn: &str) -> String {
        self.mode = AppMode::Monitoring;
        self.auto_follow = true;
        self.selected_step = 0;
        self.log_viewer = LogViewerState::new();
        self.active_tab = MonitorTab::Logs;
        self.metrics_state = MetricsState::new();
        self.selected_step_name().unwrap_or_default().to_string()
    }

    /// Transition back to execution selection.
    pub fn enter_select_execution(&mut self) {
        self.mode = AppMode::SelectExecution;
        self.execution_cursor = 0;
        self.error_message = None;
    }

    /// Transition back to pipeline selection.
    pub fn enter_select_pipeline(&mut self) {
        self.mode = AppMode::SelectPipeline;
        self.error_message = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::execution::{ExecutionStatus, PipelineExecution};
    use crate::model::logs::LogStreamState;
    use crate::model::pipeline::PipelineSummary;
    use crate::model::step::{StepInfo, StepStatus, StepType};

    fn make_step(name: &str, status: StepStatus) -> StepInfo {
        StepInfo {
            name: name.to_string(),
            step_type: StepType::Training,
            status,
            start_time: None,
            end_time: None,
            failure_reason: None,
            job_details: None,
        }
    }

    fn make_pipeline(name: &str) -> PipelineSummary {
        PipelineSummary {
            name: name.to_string(),
            description: None,
            last_execution_time: None,
        }
    }

    fn make_execution(arn: &str) -> ExecutionSummary {
        ExecutionSummary {
            arn: arn.to_string(),
            display_name: None,
            status: ExecutionStatus::Succeeded,
            start_time: None,
        }
    }

    fn make_monitoring_update(steps: Vec<StepInfo>) -> MonitoringUpdate {
        MonitoringUpdate {
            execution: PipelineExecution {
                display_name: None,
                status: ExecutionStatus::Executing,
                created: None,
                last_modified: None,
                parameters: std::collections::BTreeMap::new(),
            },
            steps,
            log_step_name: None,
            log_stream_state: None,
            metrics_step_name: None,
            metrics: None,
        }
    }

    // --- Pipeline cursor ---

    #[test]
    fn pipeline_cursor_stays_at_zero() {
        let mut app = App::new();
        app.pipelines = vec![make_pipeline("a"), make_pipeline("b")];
        app.pipeline_cursor_up();
        assert_eq!(app.pipeline_cursor, 0);
    }

    #[test]
    fn pipeline_cursor_moves_down_and_clamps() {
        let mut app = App::new();
        app.pipelines = vec![make_pipeline("a"), make_pipeline("b")];
        app.pipeline_cursor_down();
        assert_eq!(app.pipeline_cursor, 1);
        app.pipeline_cursor_down();
        assert_eq!(app.pipeline_cursor, 1); // clamped
    }

    #[test]
    fn selected_pipeline_name_returns_correct() {
        let mut app = App::new();
        app.pipelines = vec![make_pipeline("alpha"), make_pipeline("beta")];
        app.pipeline_cursor = 1;
        assert_eq!(app.selected_pipeline_name(), Some("beta"));
    }

    #[test]
    fn selected_pipeline_name_empty() {
        let app = App::new();
        assert_eq!(app.selected_pipeline_name(), None);
    }

    // --- Execution cursor ---

    #[test]
    fn execution_cursor_bounds() {
        let mut app = App::new();
        app.executions = vec![make_execution("a"), make_execution("b")];
        app.execution_cursor_up();
        assert_eq!(app.execution_cursor, 0);
        app.execution_cursor_down();
        assert_eq!(app.execution_cursor, 1);
        app.execution_cursor_down();
        assert_eq!(app.execution_cursor, 1);
    }

    #[test]
    fn selected_execution_arn() {
        let mut app = App::new();
        app.executions = vec![make_execution("arn:1"), make_execution("arn:2")];
        app.execution_cursor = 1;
        assert_eq!(app.selected_execution_arn(), Some("arn:2"));
    }

    // --- Step cursor ---

    #[test]
    fn step_cursor_navigation() {
        let mut app = App::new();
        app.steps = vec![
            make_step("a", StepStatus::Succeeded),
            make_step("b", StepStatus::Succeeded),
            make_step("c", StepStatus::Succeeded),
        ];
        app.select_step_up(); // at 0, should stay
        assert_eq!(app.selected_step, 0);
        app.select_step_down();
        assert_eq!(app.selected_step, 1);
        assert!(!app.auto_follow); // disabled on manual nav
    }

    #[test]
    fn selected_step_name_empty_steps() {
        let app = App::new();
        assert_eq!(app.selected_step_name(), None);
    }

    // --- update_steps ---

    #[test]
    fn update_steps_preserves_selection_by_name() {
        let mut app = App::new();
        app.steps = vec![
            make_step("a", StepStatus::Succeeded),
            make_step("b", StepStatus::Executing),
        ];
        app.selected_step = 1; // "b"

        // New steps reorder — "b" is now at index 0
        app.update_steps(vec![
            make_step("b", StepStatus::Succeeded),
            make_step("a", StepStatus::Succeeded),
            make_step("c", StepStatus::Executing),
        ]);
        assert_eq!(app.selected_step, 0); // followed "b"
    }

    #[test]
    fn update_steps_clamps_on_shrink() {
        let mut app = App::new();
        app.steps = vec![
            make_step("a", StepStatus::Succeeded),
            make_step("b", StepStatus::Succeeded),
            make_step("c", StepStatus::Succeeded),
        ];
        app.selected_step = 2;
        // "c" disappears
        app.update_steps(vec![make_step("x", StepStatus::Executing)]);
        assert_eq!(app.selected_step, 0);
    }

    #[test]
    fn update_steps_handles_empty() {
        let mut app = App::new();
        app.steps = vec![make_step("a", StepStatus::Succeeded)];
        app.selected_step = 0;
        app.update_steps(vec![]);
        assert_eq!(app.selected_step, 0);
        assert!(app.steps.is_empty());
    }

    // --- maybe_follow_executing_step ---

    #[test]
    fn follow_executing_step_when_enabled() {
        let mut app = App::new();
        app.auto_follow = true;
        app.steps = vec![
            make_step("a", StepStatus::Succeeded),
            make_step("b", StepStatus::Executing),
        ];
        app.selected_step = 0;
        app.maybe_follow_executing_step();
        assert_eq!(app.selected_step, 1);
    }

    #[test]
    fn follow_executing_step_disabled() {
        let mut app = App::new();
        app.auto_follow = false;
        app.steps = vec![
            make_step("a", StepStatus::Succeeded),
            make_step("b", StepStatus::Executing),
        ];
        app.selected_step = 0;
        app.maybe_follow_executing_step();
        assert_eq!(app.selected_step, 0); // unchanged
    }

    // --- Mode transitions ---

    #[test]
    fn enter_monitoring_resets_state() {
        let mut app = App::new();
        app.steps = vec![make_step("s1", StepStatus::Executing)];
        app.selected_step = 0;
        let name = app.enter_monitoring("arn:test");
        assert_eq!(app.mode, AppMode::Monitoring);
        assert!(app.auto_follow);
        assert_eq!(name, "s1");
    }

    #[test]
    fn enter_select_execution_resets() {
        let mut app = App::new();
        app.mode = AppMode::Monitoring;
        app.execution_cursor = 5;
        app.error_message = Some("err".to_string());
        app.enter_select_execution();
        assert_eq!(app.mode, AppMode::SelectExecution);
        assert_eq!(app.execution_cursor, 0);
        assert!(app.error_message.is_none());
    }

    #[test]
    fn enter_select_pipeline_resets() {
        let mut app = App::new();
        app.mode = AppMode::SelectExecution;
        app.error_message = Some("err".to_string());
        app.enter_select_pipeline();
        assert_eq!(app.mode, AppMode::SelectPipeline);
        assert!(app.error_message.is_none());
    }

    // --- apply_monitoring_result ---

    #[test]
    fn apply_monitoring_result_updates_state() {
        let mut app = App::new();
        app.mode = AppMode::Monitoring;
        app.auto_follow = true;

        let steps = vec![
            make_step("a", StepStatus::Succeeded),
            make_step("b", StepStatus::Executing),
        ];
        let update = make_monitoring_update(steps);
        app.apply_monitoring_result(update);

        assert!(app.execution.is_some());
        assert_eq!(app.steps.len(), 2);
        assert_eq!(app.selected_step, 1); // followed executing
        assert!(app.last_successful_poll.is_some());
        assert!(app.last_poll_error.is_none());
    }

    #[test]
    fn apply_monitoring_result_inserts_log_cache() {
        let mut app = App::new();
        app.mode = AppMode::Monitoring;

        let mut update = make_monitoring_update(vec![make_step("s1", StepStatus::Executing)]);
        update.log_step_name = Some("s1".to_string());
        update.log_stream_state = Some(LogStreamState::new("/log/group".to_string()));

        app.apply_monitoring_result(update);
        assert!(app.log_viewer.per_step_cache.contains_key("s1"));
    }

    #[test]
    fn apply_monitoring_result_leaves_executions_alone() {
        let mut app = App::new();
        app.executions = vec![make_execution("arn:1"), make_execution("arn:2")];
        app.execution_cursor = 1;

        let update = make_monitoring_update(vec![make_step("s1", StepStatus::Executing)]);
        app.apply_monitoring_result(update);

        assert_eq!(app.executions.len(), 2);
        assert_eq!(app.execution_cursor, 1);
    }

    // --- apply_execution_list_result ---

    #[test]
    fn apply_execution_list_result_replaces_list() {
        let mut app = App::new();
        app.selected_pipeline_name = Some("p1".to_string());
        app.executions = vec![make_execution("old")];
        app.execution_cursor = 0;

        app.apply_execution_list_result("p1", vec![make_execution("new1"), make_execution("new2")]);

        assert_eq!(app.executions.len(), 2);
        assert_eq!(app.executions[0].arn, "new1");
        assert_eq!(app.execution_cursor, 0);
        assert!(app.last_successful_poll.is_some());
    }

    #[test]
    fn apply_execution_list_result_clamps_cursor_on_shrink() {
        let mut app = App::new();
        app.selected_pipeline_name = Some("p1".to_string());
        app.executions = vec![
            make_execution("a"),
            make_execution("b"),
            make_execution("c"),
        ];
        app.execution_cursor = 2;

        app.apply_execution_list_result("p1", vec![make_execution("x")]);

        assert_eq!(app.execution_cursor, 0);
    }

    #[test]
    fn apply_execution_list_result_preserves_cursor_when_long_enough() {
        let mut app = App::new();
        app.selected_pipeline_name = Some("p1".to_string());
        app.executions = vec![make_execution("a"), make_execution("b")];
        app.execution_cursor = 1;

        app.apply_execution_list_result(
            "p1",
            vec![
                make_execution("a"),
                make_execution("b"),
                make_execution("c"),
            ],
        );

        assert_eq!(app.execution_cursor, 1);
    }

    #[test]
    fn apply_execution_list_result_handles_empty() {
        let mut app = App::new();
        app.selected_pipeline_name = Some("p1".to_string());
        app.executions = vec![make_execution("a")];
        app.execution_cursor = 0;

        app.apply_execution_list_result("p1", vec![]);

        assert!(app.executions.is_empty());
        assert_eq!(app.execution_cursor, 0);
    }

    #[test]
    fn apply_execution_list_result_ignores_mismatched_pipeline() {
        let mut app = App::new();
        app.selected_pipeline_name = Some("p1".to_string());
        app.executions = vec![make_execution("kept")];

        app.apply_execution_list_result("p2", vec![make_execution("other")]);

        // List unchanged
        assert_eq!(app.executions.len(), 1);
        assert_eq!(app.executions[0].arn, "kept");
        // But counted as healthy
        assert!(app.last_successful_poll.is_some());
    }

    #[test]
    fn apply_execution_list_result_does_not_touch_monitoring_state() {
        let mut app = App::new();
        app.selected_pipeline_name = Some("p1".to_string());
        app.steps = vec![make_step("s1", StepStatus::Executing)];
        app.mode = AppMode::Monitoring;

        app.apply_execution_list_result("p1", vec![make_execution("a")]);

        assert_eq!(app.steps.len(), 1);
        assert_eq!(app.mode, AppMode::Monitoring);
        assert!(app.execution.is_none());
    }

    // --- error state + stale timer ---

    #[test]
    fn poll_error_sets_last_poll_error() {
        let mut app = App::new();
        app.apply_poll_result(PollResult::Error(PollError::CredentialsExpired {
            message: "ExpiredToken".to_string(),
        }));
        assert!(matches!(
            app.last_poll_error,
            Some(PollError::CredentialsExpired { .. })
        ));
    }

    #[test]
    fn monitoring_success_clears_error() {
        let mut app = App::new();
        app.last_poll_error = Some(PollError::Other {
            message: "boom".to_string(),
        });
        let update = make_monitoring_update(vec![make_step("s1", StepStatus::Executing)]);
        app.apply_monitoring_result(update);
        assert!(app.last_poll_error.is_none());
        assert!(app.last_successful_poll.is_some());
    }

    #[test]
    fn execution_list_success_clears_error() {
        let mut app = App::new();
        app.selected_pipeline_name = Some("p1".to_string());
        app.last_poll_error = Some(PollError::Other {
            message: "boom".to_string(),
        });
        app.apply_execution_list_result("p1", vec![make_execution("a")]);
        assert!(app.last_poll_error.is_none());
        assert!(app.last_successful_poll.is_some());
    }

    #[test]
    fn stale_level_none_is_stale() {
        let now = Utc::now();
        assert_eq!(stale_level(None, now), StaleLevel::Stale);
    }

    #[test]
    fn stale_level_recent_is_fresh() {
        let now = Utc::now();
        let t = now - chrono::Duration::seconds(5);
        assert_eq!(stale_level(Some(t), now), StaleLevel::Fresh);
    }

    #[test]
    fn stale_level_at_threshold_is_fresh() {
        let now = Utc::now();
        let t = now - chrono::Duration::seconds(STALE_THRESHOLD_SECS);
        assert_eq!(stale_level(Some(t), now), StaleLevel::Fresh);
    }

    #[test]
    fn stale_level_past_threshold_is_stale() {
        let now = Utc::now();
        let t = now - chrono::Duration::seconds(STALE_THRESHOLD_SECS + 1);
        assert_eq!(stale_level(Some(t), now), StaleLevel::Stale);
    }

    // --- parameter editor ---

    fn make_params() -> Vec<PipelineParameter> {
        vec![
            PipelineParameter {
                name: "batch_size".to_string(),
                type_name: "Integer".to_string(),
                default_value: Some("32".to_string()),
                initial_value: "32".to_string(),
            },
            PipelineParameter {
                name: "model".to_string(),
                type_name: "String".to_string(),
                default_value: Some("resnet50".to_string()),
                initial_value: "resnet50".to_string(),
            },
        ]
    }

    #[test]
    fn open_parameter_editor_sets_loading() {
        let mut app = App::new();
        app.open_parameter_editor("p1".to_string());
        let editor = app.parameter_editor.as_ref().unwrap();
        assert!(editor.loading);
        assert_eq!(editor.pipeline_name, "p1");
        assert!(editor.parameters.is_empty());
    }

    #[test]
    fn populate_parameter_editor_fills_defaults() {
        let mut app = App::new();
        app.open_parameter_editor("p1".to_string());
        app.populate_parameter_editor(make_params());
        let editor = app.parameter_editor.as_ref().unwrap();
        assert!(!editor.loading);
        assert_eq!(
            editor.values,
            vec!["32".to_string(), "resnet50".to_string()]
        );
        assert_eq!(editor.cursor, 0);
    }

    #[test]
    fn parameter_editor_cursor_bounds() {
        let mut app = App::new();
        app.open_parameter_editor("p1".to_string());
        app.populate_parameter_editor(make_params());
        app.parameter_editor_cursor_up();
        assert_eq!(app.parameter_editor.as_ref().unwrap().cursor, 0);
        app.parameter_editor_cursor_down();
        assert_eq!(app.parameter_editor.as_ref().unwrap().cursor, 1);
        app.parameter_editor_cursor_down();
        assert_eq!(app.parameter_editor.as_ref().unwrap().cursor, 1);
    }

    #[test]
    fn parameter_editor_input_appends_to_selected_only() {
        let mut app = App::new();
        app.open_parameter_editor("p1".to_string());
        app.populate_parameter_editor(make_params());
        app.parameter_editor_cursor_down();
        app.parameter_editor_input('x');
        let editor = app.parameter_editor.as_ref().unwrap();
        assert_eq!(editor.values[0], "32");
        assert_eq!(editor.values[1], "resnet50x");
    }

    #[test]
    fn parameter_editor_backspace_removes_from_selected() {
        let mut app = App::new();
        app.open_parameter_editor("p1".to_string());
        app.populate_parameter_editor(make_params());
        app.parameter_editor_backspace();
        assert_eq!(app.parameter_editor.as_ref().unwrap().values[0], "3");
    }

    #[test]
    fn parameter_editor_clear_field_empties_selected() {
        let mut app = App::new();
        app.open_parameter_editor("p1".to_string());
        app.populate_parameter_editor(make_params());
        app.parameter_editor_clear_field();
        assert_eq!(app.parameter_editor.as_ref().unwrap().values[0], "");
    }

    #[test]
    fn close_parameter_editor_clears_state() {
        let mut app = App::new();
        app.open_parameter_editor("p1".to_string());
        app.close_parameter_editor();
        assert!(app.parameter_editor.is_none());
    }

    #[test]
    fn parameter_editor_overrides_only_includes_changes() {
        let mut app = App::new();
        app.open_parameter_editor("p1".to_string());
        app.populate_parameter_editor(make_params());
        app.parameter_editor_input('0'); // "32" -> "320"
        let editor = app.parameter_editor.as_ref().unwrap();
        let overrides = editor.overrides();
        assert_eq!(
            overrides,
            vec![("batch_size".to_string(), "320".to_string())]
        );
    }

    #[test]
    fn parameter_editor_overrides_always_includes_required() {
        let mut app = App::new();
        app.open_parameter_editor("p1".to_string());
        app.populate_parameter_editor(vec![
            PipelineParameter {
                name: "optional_with_default".to_string(),
                type_name: "String".to_string(),
                default_value: Some("keep".to_string()),
                initial_value: "keep".to_string(),
            },
            PipelineParameter {
                name: "required_pre_filled".to_string(),
                type_name: "String".to_string(),
                default_value: None,
                initial_value: "latest-run-value".to_string(),
            },
        ]);
        let overrides = app.parameter_editor.as_ref().unwrap().overrides();
        assert_eq!(
            overrides,
            vec![(
                "required_pre_filled".to_string(),
                "latest-run-value".to_string()
            )]
        );
    }

    #[test]
    fn populate_parameter_editor_uses_initial_value_for_required() {
        let mut app = App::new();
        app.open_parameter_editor("p1".to_string());
        app.populate_parameter_editor(vec![PipelineParameter {
            name: "token".to_string(),
            type_name: "String".to_string(),
            default_value: None,
            initial_value: "from-last-run".to_string(),
        }]);
        assert_eq!(
            app.parameter_editor.as_ref().unwrap().values,
            vec!["from-last-run".to_string()]
        );
    }
}
