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
        _ => Action::None,
    }
}
