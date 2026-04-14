use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppMode, MonitorTab};

/// Describes a side-effect the main loop should perform after handling a key.
pub enum Action {
    None,
    Quit,
    LoadExecutions {
        pipeline_name: String,
    },
    StartMonitoring {
        arn: String,
        step_name: String,
    },
    StepChanged {
        step_name: String,
    },
    BackToExecutions {
        pipeline_name: String,
    },
    BackToPipelines,
    ToggleNotifications,
    StopPipeline,
    RetryPipeline,
    OpenStartExecutionEditor {
        pipeline_name: String,
    },
    SubmitStartExecution {
        pipeline_name: String,
        overrides: Vec<(String, String)>,
    },
    CancelStartExecution,
}

/// Pure key handler: mutates App state and returns an Action for the caller.
/// No async, no channels, no AWS clients.
pub fn handle_key(app: &mut App, key: KeyEvent, has_pipeline_flag: bool) -> Action {
    // Ctrl+C always quits
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Action::Quit;
    }

    // Parameter editor overlay captures keys before anything else so letters
    // (including `q`) can be typed as parameter values.
    if app.parameter_editor.is_some() {
        return handle_parameter_editor(app, key);
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

fn handle_parameter_editor(app: &mut App, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => Action::CancelStartExecution,
        KeyCode::Up => {
            app.parameter_editor_cursor_up();
            Action::None
        }
        KeyCode::Down => {
            app.parameter_editor_cursor_down();
            Action::None
        }
        KeyCode::Enter => {
            let Some(editor) = app.parameter_editor.as_ref() else {
                return Action::None;
            };
            if editor.loading {
                return Action::None;
            }
            Action::SubmitStartExecution {
                pipeline_name: editor.pipeline_name.clone(),
                overrides: editor.overrides(),
            }
        }
        KeyCode::Backspace => {
            app.parameter_editor_backspace();
            Action::None
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.parameter_editor_clear_field();
            Action::None
        }
        KeyCode::Char(c) => {
            app.parameter_editor_input(c);
            Action::None
        }
        _ => Action::None,
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
                Action::LoadExecutions {
                    pipeline_name: name,
                }
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
                Action::BackToPipelines
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
        KeyCode::Char('N') => {
            if let Some(name) = app.selected_pipeline_name.clone() {
                Action::OpenStartExecutionEditor {
                    pipeline_name: name,
                }
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
        KeyCode::Tab => {
            app.toggle_tab();
            Action::None
        }
        KeyCode::Char('j') => {
            match app.active_tab {
                MonitorTab::Logs => {
                    let name = app.selected_step_name().unwrap_or_default().to_string();
                    app.log_viewer.scroll_down(&name, 3);
                }
                MonitorTab::Metrics => {
                    let max = metrics_series_count(app);
                    app.metrics_state.cursor_down(max);
                }
            }
            Action::None
        }
        KeyCode::Char('k') => {
            match app.active_tab {
                MonitorTab::Logs => {
                    app.log_viewer.scroll_up(3);
                }
                MonitorTab::Metrics => {
                    app.metrics_state.cursor_up();
                }
            }
            Action::None
        }
        KeyCode::Char(' ') => {
            if app.active_tab == MonitorTab::Metrics {
                let names = metrics_series_names(app);
                if let Some(name) = names.get(app.metrics_state.metrics_cursor) {
                    let name = name.clone();
                    app.metrics_state.toggle_metric(&name);
                }
            }
            Action::None
        }
        KeyCode::Char('a') => {
            if app.active_tab == MonitorTab::Metrics {
                let names = metrics_series_names(app);
                app.metrics_state.toggle_all(&names);
            }
            Action::None
        }
        KeyCode::Char('G') => {
            if app.active_tab == MonitorTab::Logs {
                let name = app.selected_step_name().unwrap_or_default().to_string();
                app.log_viewer.jump_to_end(&name);
            }
            Action::None
        }
        KeyCode::Char('g') => {
            if app.active_tab == MonitorTab::Logs {
                app.log_viewer.jump_to_start();
            }
            Action::None
        }
        KeyCode::Char('n') => Action::ToggleNotifications,
        KeyCode::Char('S') => Action::StopPipeline,
        KeyCode::Char('R') => Action::RetryPipeline,
        _ => Action::None,
    }
}

fn metrics_series_names(app: &App) -> Vec<String> {
    let step_name = app.selected_step_name().unwrap_or_default();
    app.metrics_state
        .metrics_for_step(step_name)
        .map(|m| {
            m.experiment_series
                .iter()
                .filter(|s| !s.points.is_empty())
                .map(|s| s.metric_name.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn metrics_series_count(app: &App) -> usize {
    metrics_series_names(app).len()
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
        for mode in [
            AppMode::SelectPipeline,
            AppMode::SelectExecution,
            AppMode::Monitoring,
        ] {
            let mut app = App::new();
            app.mode = mode;
            assert!(matches!(
                handle_key(&mut app, ctrl_c(), false),
                Action::Quit
            ));
        }
    }

    #[test]
    fn q_quits_from_all_modes() {
        for mode in [
            AppMode::SelectPipeline,
            AppMode::SelectExecution,
            AppMode::Monitoring,
        ] {
            let mut app = App::new();
            app.mode = mode;
            assert!(matches!(
                handle_key(&mut app, key(KeyCode::Char('q')), false),
                Action::Quit
            ));
        }
    }

    // --- SelectPipeline ---

    #[test]
    fn select_pipeline_esc_quits() {
        let mut app = App::new();
        app.mode = AppMode::SelectPipeline;
        assert!(matches!(
            handle_key(&mut app, key(KeyCode::Esc), false),
            Action::Quit
        ));
    }

    #[test]
    fn select_pipeline_up_down() {
        let mut app = App::new();
        app.mode = AppMode::SelectPipeline;
        app.pipelines = vec![
            PipelineSummary {
                name: "a".into(),
                description: None,
                last_execution_time: None,
            },
            PipelineSummary {
                name: "b".into(),
                description: None,
                last_execution_time: None,
            },
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
        app.pipelines = vec![PipelineSummary {
            name: "my-pipe".into(),
            description: None,
            last_execution_time: None,
        }];
        let action = handle_key(&mut app, key(KeyCode::Enter), false);
        assert!(
            matches!(action, Action::LoadExecutions { pipeline_name } if pipeline_name == "my-pipe")
        );
        assert_eq!(app.mode, AppMode::SelectExecution);
    }

    #[test]
    fn select_pipeline_enter_empty_is_none() {
        let mut app = App::new();
        app.mode = AppMode::SelectPipeline;
        assert!(matches!(
            handle_key(&mut app, key(KeyCode::Enter), false),
            Action::None
        ));
    }

    // --- SelectExecution ---

    #[test]
    fn select_execution_esc_goes_back_no_flag() {
        let mut app = App::new();
        app.mode = AppMode::SelectExecution;
        let action = handle_key(&mut app, key(KeyCode::Esc), false);
        assert!(matches!(action, Action::BackToPipelines));
        assert_eq!(app.mode, AppMode::SelectPipeline);
    }

    #[test]
    fn select_execution_esc_quits_with_flag() {
        let mut app = App::new();
        app.mode = AppMode::SelectExecution;
        assert!(matches!(
            handle_key(&mut app, key(KeyCode::Esc), true),
            Action::Quit
        ));
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
        assert!(
            matches!(action, Action::BackToExecutions { pipeline_name } if pipeline_name == "pipe")
        );
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
    fn monitoring_s_stops_pipeline() {
        let mut app = App::new();
        app.mode = AppMode::Monitoring;
        assert!(matches!(
            handle_key(&mut app, key(KeyCode::Char('S')), false),
            Action::StopPipeline
        ));
    }

    #[test]
    fn monitoring_r_restarts_pipeline() {
        let mut app = App::new();
        app.mode = AppMode::Monitoring;
        assert!(matches!(
            handle_key(&mut app, key(KeyCode::Char('R')), false),
            Action::RetryPipeline
        ));
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
