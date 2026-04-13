pub mod execution_list;
pub mod header;
pub mod logs;
pub mod metrics;
pub mod parameter_editor;
pub mod pipeline_list;
pub mod status_bar;
pub mod steps;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, AppMode, MonitorTab};
use crate::polling::PollError;

pub fn draw(f: &mut Frame, app: &mut App) {
    match app.mode {
        AppMode::SelectPipeline => draw_pipeline_list(f, app),
        AppMode::SelectExecution => draw_execution_list(f, app),
        AppMode::Monitoring => draw_monitor(f, app),
    }

    if app.parameter_editor.is_some() {
        parameter_editor::draw(f, app);
    }
}

/// Split off a 1-row banner at the top of `area` if a poll error is present.
/// Returns `(banner_area, rest_area)`; when there's no error, the banner area
/// is zero-height and the full original area is returned as `rest`.
fn split_error_banner(area: Rect, has_error: bool) -> (Rect, Rect) {
    if !has_error {
        return (
            Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: 0,
            },
            area,
        );
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);
    (chunks[0], chunks[1])
}

fn draw_error_banner(f: &mut Frame, area: Rect, err: &PollError) {
    let text = match err {
        PollError::CredentialsExpired { .. } => {
            " AWS credentials expired — re-authenticate and restart sagecue ".to_string()
        }
        PollError::Other { .. } => {
            format!(" Poll error: {} ", err.message())
        }
    };
    let para = Paragraph::new(text).style(
        Style::default()
            .fg(Color::White)
            .bg(Color::Red)
            .add_modifier(Modifier::BOLD),
    );
    f.render_widget(para, area);
}

fn draw_pipeline_list(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // pipeline list
            Constraint::Length(1), // status bar
        ])
        .split(area);

    pipeline_list::draw(f, app, chunks[0]);
    status_bar::draw_pipeline_list_bar(f, chunks[1]);
}

fn draw_execution_list(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let (banner, rest) = split_error_banner(area, app.last_poll_error.is_some());
    if let Some(ref err) = app.last_poll_error {
        draw_error_banner(f, banner, err);
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // execution list
            Constraint::Length(1), // status bar
        ])
        .split(rest);

    execution_list::draw(f, app, chunks[0]);
    status_bar::draw_execution_list_bar(f, chunks[1]);
}

fn draw_monitor(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let (banner, rest) = split_error_banner(area, app.last_poll_error.is_some());
    if let Some(ref err) = app.last_poll_error {
        draw_error_banner(f, banner, err);
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),                                 // header
            Constraint::Length(6 + app.steps.len().max(1) as u16), // step table
            Constraint::Min(5),                                    // logs
            Constraint::Length(1),                                 // status bar
        ])
        .split(rest);

    header::draw(f, app, chunks[0]);
    steps::draw(f, app, chunks[1]);
    match app.active_tab {
        MonitorTab::Logs => logs::draw(f, app, chunks[2]),
        MonitorTab::Metrics => metrics::draw(f, app, chunks[2]),
    }
    let is_executing = app
        .execution
        .as_ref()
        .map(|e| e.status == crate::model::execution::ExecutionStatus::Executing)
        .unwrap_or(false);
    status_bar::draw_monitor_bar(
        f,
        chunks[3],
        app.notifications_enabled,
        app.background_watcher_count,
        is_executing,
        app.active_tab,
    );
}
