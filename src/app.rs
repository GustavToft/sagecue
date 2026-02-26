use crate::model::execution::{ExecutionSummary, PipelineExecution};
use crate::model::logs::LogViewerState;
use crate::model::pipeline::PipelineSummary;
use crate::model::step::{StepInfo, StepStatus, PIPELINE_STEPS};

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
}

impl App {
    pub fn new() -> Self {
        let steps = PIPELINE_STEPS.iter().map(|&name| StepInfo::new(name)).collect();
        Self {
            mode: AppMode::SelectPipeline,
            pipelines: Vec::new(),
            pipeline_cursor: 0,
            selected_pipeline_name: None,
            executions: Vec::new(),
            execution_cursor: 0,
            execution: None,
            steps,
            selected_step: 0,
            auto_follow: true,
            log_viewer: LogViewerState::new(),
            should_quit: false,
            error_message: None,
            loading: true,
        }
    }

    pub fn selected_step_name(&self) -> &str {
        &self.steps[self.selected_step].name
    }

    pub fn select_step_up(&mut self) {
        if self.selected_step > 0 {
            self.selected_step -= 1;
            self.auto_follow = false;
            self.on_step_changed();
        }
    }

    pub fn select_step_down(&mut self) {
        if self.selected_step < self.steps.len() - 1 {
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

    fn on_step_changed(&mut self) {
        // Reset scroll to end for the new step
        let step_name = self.steps[self.selected_step].name.clone();
        self.log_viewer.jump_to_end(&step_name);
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
}
