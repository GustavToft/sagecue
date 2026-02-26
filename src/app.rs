use crate::model::execution::{ExecutionSummary, PipelineExecution};
use crate::model::logs::LogViewerState;
use crate::model::pipeline::PipelineSummary;
use crate::model::step::{StepInfo, StepStatus};
use crate::notify;
use crate::polling::PollResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    SelectPipeline,
    SelectExecution,
    Monitoring,
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
    pub should_quit: bool,
    pub error_message: Option<String>,
    pub loading: bool,
    pub notifications_enabled: bool,
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
            should_quit: false,
            error_message: None,
            loading: true,
            notifications_enabled: false,
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

    pub fn toggle_notifications(&mut self) {
        self.notifications_enabled = !self.notifications_enabled;
    }

    /// Apply a poll result from the background polling task.
    pub fn apply_poll_result(&mut self, result: PollResult) {
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
            self.log_viewer.per_step_cache.insert(step_name, stream_state);
        }
    }

    /// Transition into monitoring mode. Returns the initial step name for the poller.
    pub fn enter_monitoring(&mut self, _arn: &str) -> String {
        self.mode = AppMode::Monitoring;
        self.auto_follow = true;
        self.selected_step = 0;
        self.log_viewer = LogViewerState::new();
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

    fn make_poll_result(steps: Vec<StepInfo>) -> PollResult {
        PollResult {
            execution: PipelineExecution {
                display_name: None,
                status: ExecutionStatus::Executing,
                created: None,
                last_modified: None,
            },
            steps,
            log_step_name: None,
            log_stream_state: None,
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

    // --- apply_poll_result ---

    #[test]
    fn apply_poll_result_updates_state() {
        let mut app = App::new();
        app.mode = AppMode::Monitoring;
        app.auto_follow = true;

        let steps = vec![
            make_step("a", StepStatus::Succeeded),
            make_step("b", StepStatus::Executing),
        ];
        let result = make_poll_result(steps);
        app.apply_poll_result(result);

        assert!(app.execution.is_some());
        assert_eq!(app.steps.len(), 2);
        assert_eq!(app.selected_step, 1); // followed executing
    }

    #[test]
    fn apply_poll_result_inserts_log_cache() {
        let mut app = App::new();
        app.mode = AppMode::Monitoring;

        let mut result = make_poll_result(vec![make_step("s1", StepStatus::Executing)]);
        result.log_step_name = Some("s1".to_string());
        result.log_stream_state = Some(LogStreamState::new("/log/group".to_string()));

        app.apply_poll_result(result);
        assert!(app.log_viewer.per_step_cache.contains_key("s1"));
    }
}
