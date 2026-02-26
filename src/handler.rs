use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppMode};

/// Describes a side-effect the main loop should perform after handling a key.
pub enum Action {
    None,
    Quit,
    LoadExecutions { pipeline_name: String },
    StartMonitoring { arn: String, step_name: String },
    ForceRefresh,
    StepChanged { step_name: String },
    BackToExecutions { pipeline_name: String },
    ToggleNotifications,
}

/// Pure key handler: mutates App state and returns an Action for the caller.
/// No async, no channels, no AWS clients.
pub fn handle_key(app: &mut App, key: KeyEvent, has_pipeline_flag: bool) -> Action {
    // Ctrl+C always quits
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Action::Quit;
    }

    // q always quits
    if key.code == KeyCode::Char('q') {
        return Action::Quit;
    }

    match app.mode {
        AppMode::SelectPipeline => handle_select_pipeline(app, key),
        AppMode::SelectExecution => handle_select_execution(app, key, has_pipeline_flag),
        AppMode::Monitoring => handle_monitoring(app, key),
    }
}

fn handle_select_pipeline(app: &mut App, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => Action::Quit,
        KeyCode::Up => {
            app.pipeline_cursor_up();
            Action::None
        }
        KeyCode::Down => {
            app.pipeline_cursor_down();
            Action::None
        }
        KeyCode::Enter => {
            if let Some(name) = app.selected_pipeline_name() {
                let name = name.to_string();
                app.selected_pipeline_name = Some(name.clone());
                app.mode = AppMode::SelectExecution;
                app.loading = true;
                app.execution_cursor = 0;
                app.error_message = None;
                Action::LoadExecutions { pipeline_name: name }
            } else {
                Action::None
            }
        }
        _ => Action::None,
    }
}

fn handle_select_execution(app: &mut App, key: KeyEvent, has_pipeline_flag: bool) -> Action {
    match key.code {
        KeyCode::Esc => {
            if !has_pipeline_flag {
                app.enter_select_pipeline();
                Action::None
            } else {
                Action::Quit
            }
        }
        KeyCode::Up => {
            app.execution_cursor_up();
            Action::None
        }
        KeyCode::Down => {
            app.execution_cursor_down();
            Action::None
        }
        KeyCode::Enter => {
            if let Some(arn) = app.selected_execution_arn() {
                let arn = arn.to_string();
                let step_name = app.enter_monitoring(&arn);
                Action::StartMonitoring { arn, step_name }
            } else {
                Action::None
            }
        }
        _ => Action::None,
    }
}

fn handle_monitoring(app: &mut App, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            let pipeline_name = app.selected_pipeline_name.clone().unwrap_or_default();
            app.enter_select_execution();
            Action::BackToExecutions { pipeline_name }
        }
        KeyCode::Up => {
            app.select_step_up();
            let name = app.selected_step_name().unwrap_or_default().to_string();
            Action::StepChanged { step_name: name }
        }
        KeyCode::Down => {
            app.select_step_down();
            let name = app.selected_step_name().unwrap_or_default().to_string();
            Action::StepChanged { step_name: name }
        }
        KeyCode::Char('j') => {
            let name = app.selected_step_name().unwrap_or_default().to_string();
            app.log_viewer.scroll_down(&name, 3);
            Action::None
        }
        KeyCode::Char('k') => {
            app.log_viewer.scroll_up(3);
            Action::None
        }
        KeyCode::Char('G') => {
            let name = app.selected_step_name().unwrap_or_default().to_string();
            app.log_viewer.jump_to_end(&name);
            Action::None
        }
        KeyCode::Char('g') => {
            app.log_viewer.jump_to_start();
            Action::None
        }
        KeyCode::Char('r') => Action::ForceRefresh,
        KeyCode::Char('n') => Action::ToggleNotifications,
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::execution::{ExecutionStatus, ExecutionSummary};
    use crate::model::pipeline::PipelineSummary;
    use crate::model::step::{StepInfo, StepStatus, StepType};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_c() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
    }

    fn make_step(name: &str) -> StepInfo {
        StepInfo {
            name: name.to_string(),
            step_type: StepType::Training,
            status: StepStatus::Succeeded,
            start_time: None,
            end_time: None,
            failure_reason: None,
            job_details: None,
        }
    }

    // --- Global quit ---

    #[test]
    fn ctrl_c_quits_from_all_modes() {
        for mode in [AppMode::SelectPipeline, AppMode::SelectExecution, AppMode::Monitoring] {
            let mut app = App::new();
            app.mode = mode;
            assert!(matches!(handle_key(&mut app, ctrl_c(), false), Action::Quit));
        }
    }

    #[test]
    fn q_quits_from_all_modes() {
        for mode in [AppMode::SelectPipeline, AppMode::SelectExecution, AppMode::Monitoring] {
            let mut app = App::new();
            app.mode = mode;
            assert!(matches!(handle_key(&mut app, key(KeyCode::Char('q')), false), Action::Quit));
        }
    }

    // --- SelectPipeline ---

    #[test]
    fn select_pipeline_esc_quits() {
        let mut app = App::new();
        app.mode = AppMode::SelectPipeline;
        assert!(matches!(handle_key(&mut app, key(KeyCode::Esc), false), Action::Quit));
    }

    #[test]
    fn select_pipeline_up_down() {
        let mut app = App::new();
        app.mode = AppMode::SelectPipeline;
        app.pipelines = vec![
            PipelineSummary { name: "a".into(), description: None, last_execution_time: None },
            PipelineSummary { name: "b".into(), description: None, last_execution_time: None },
        ];
        handle_key(&mut app, key(KeyCode::Down), false);
        assert_eq!(app.pipeline_cursor, 1);
        handle_key(&mut app, key(KeyCode::Up), false);
        assert_eq!(app.pipeline_cursor, 0);
    }

    #[test]
    fn select_pipeline_enter_transitions() {
        let mut app = App::new();
        app.mode = AppMode::SelectPipeline;
        app.pipelines = vec![
            PipelineSummary { name: "my-pipe".into(), description: None, last_execution_time: None },
        ];
        let action = handle_key(&mut app, key(KeyCode::Enter), false);
        assert!(matches!(action, Action::LoadExecutions { pipeline_name } if pipeline_name == "my-pipe"));
        assert_eq!(app.mode, AppMode::SelectExecution);
    }

    #[test]
    fn select_pipeline_enter_empty_is_none() {
        let mut app = App::new();
        app.mode = AppMode::SelectPipeline;
        assert!(matches!(handle_key(&mut app, key(KeyCode::Enter), false), Action::None));
    }

    // --- SelectExecution ---

    #[test]
    fn select_execution_esc_goes_back_no_flag() {
        let mut app = App::new();
        app.mode = AppMode::SelectExecution;
        let action = handle_key(&mut app, key(KeyCode::Esc), false);
        assert!(matches!(action, Action::None));
        assert_eq!(app.mode, AppMode::SelectPipeline);
    }

    #[test]
    fn select_execution_esc_quits_with_flag() {
        let mut app = App::new();
        app.mode = AppMode::SelectExecution;
        assert!(matches!(handle_key(&mut app, key(KeyCode::Esc), true), Action::Quit));
    }

    #[test]
    fn select_execution_enter_starts_monitoring() {
        let mut app = App::new();
        app.mode = AppMode::SelectExecution;
        app.executions = vec![ExecutionSummary {
            arn: "arn:exec:1".into(),
            display_name: None,
            status: ExecutionStatus::Succeeded,
            start_time: None,
        }];
        app.steps = vec![make_step("step1")];
        let action = handle_key(&mut app, key(KeyCode::Enter), false);
        assert!(matches!(action, Action::StartMonitoring { arn, .. } if arn == "arn:exec:1"));
        assert_eq!(app.mode, AppMode::Monitoring);
    }

    // --- Monitoring ---

    #[test]
    fn monitoring_esc_goes_back() {
        let mut app = App::new();
        app.mode = AppMode::Monitoring;
        app.selected_pipeline_name = Some("pipe".to_string());
        let action = handle_key(&mut app, key(KeyCode::Esc), false);
        assert!(matches!(action, Action::BackToExecutions { pipeline_name } if pipeline_name == "pipe"));
        assert_eq!(app.mode, AppMode::SelectExecution);
    }

    #[test]
    fn monitoring_up_down_changes_step() {
        let mut app = App::new();
        app.mode = AppMode::Monitoring;
        app.steps = vec![make_step("a"), make_step("b")];
        let action = handle_key(&mut app, key(KeyCode::Down), false);
        assert!(matches!(action, Action::StepChanged { .. }));
        assert_eq!(app.selected_step, 1);
    }

    #[test]
    fn monitoring_r_force_refresh() {
        let mut app = App::new();
        app.mode = AppMode::Monitoring;
        assert!(matches!(handle_key(&mut app, key(KeyCode::Char('r')), false), Action::ForceRefresh));
    }

    #[test]
    fn monitoring_j_k_scroll_logs() {
        let mut app = App::new();
        app.mode = AppMode::Monitoring;
        app.steps = vec![make_step("s1")];
        // j scrolls down (no crash on empty logs)
        handle_key(&mut app, key(KeyCode::Char('j')), false);
        // k scrolls up
        handle_key(&mut app, key(KeyCode::Char('k')), false);
        assert_eq!(app.log_viewer.scroll_offset, 0);
    }
}
