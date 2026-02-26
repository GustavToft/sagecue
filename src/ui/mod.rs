pub mod execution_list;
pub mod header;
pub mod logs;
pub mod pipeline_list;
pub mod status_bar;
pub mod steps;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use crate::app::{App, AppMode};

pub fn draw(f: &mut Frame, app: &App) {
    match app.mode {
        AppMode::SelectPipeline => draw_pipeline_list(f, app),
        AppMode::SelectExecution => draw_execution_list(f, app),
        AppMode::Monitoring => draw_monitor(f, app),
    }
}

fn draw_pipeline_list(f: &mut Frame, app: &App) {
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

fn draw_execution_list(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // execution list
            Constraint::Length(1), // status bar
        ])
        .split(area);

    execution_list::draw(f, app, chunks[0]);
    status_bar::draw_execution_list_bar(f, chunks[1]);
}

fn draw_monitor(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // header
            Constraint::Length(6 + app.steps.len().max(1) as u16), // step table
            Constraint::Min(5),    // logs
            Constraint::Length(1), // status bar
        ])
        .split(area);

    header::draw(f, app, chunks[0]);
    steps::draw(f, app, chunks[1]);
    logs::draw(f, app, chunks[2]);
    status_bar::draw_monitor_bar(f, chunks[3], app.notifications_enabled);
}
