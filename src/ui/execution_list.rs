use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use ratatui::Frame;
use ratatui::layout::Constraint;

use crate::app::App;
use crate::model::execution::ExecutionStatus;

fn status_color(status: &ExecutionStatus) -> Color {
    match status {
        ExecutionStatus::Executing => Color::Yellow,
        ExecutionStatus::Succeeded => Color::Green,
        ExecutionStatus::Failed => Color::Red,
        ExecutionStatus::Stopped => Color::Red,
        ExecutionStatus::Stopping => Color::Yellow,
        ExecutionStatus::Unknown(_) => Color::Gray,
    }
}

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    if app.loading && app.executions.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Pipeline Executions ")
            .border_style(Style::default().fg(Color::Cyan));
        let loading = ratatui::widgets::Paragraph::new("Loading executions...")
            .style(Style::default().fg(Color::Yellow))
            .block(block);
        f.render_widget(loading, area);
        return;
    }

    if app.executions.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Pipeline Executions ")
            .border_style(Style::default().fg(Color::Cyan));
        let msg = if let Some(ref err) = app.error_message {
            format!("Error: {}", err)
        } else {
            "No executions found".to_string()
        };
        let para = ratatui::widgets::Paragraph::new(msg)
            .style(Style::default().fg(Color::Red))
            .block(block);
        f.render_widget(para, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from("  "),
        Cell::from("Execution"),
        Cell::from("Status"),
        Cell::from("Started"),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = app
        .executions
        .iter()
        .enumerate()
        .map(|(i, exec)| {
            let is_selected = i == app.execution_cursor;
            let color = status_color(&exec.status);

            let prefix = if is_selected { "> " } else { "  " };
            let name = exec
                .display_name
                .as_deref()
                .unwrap_or("<unnamed>");
            let started = exec
                .start_time
                .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "--".to_string());

            let style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(prefix.to_string()),
                Cell::from(name.to_string()),
                Cell::from(exec.status.as_str().to_string()).style(Style::default().fg(color)),
                Cell::from(started),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(2),
        Constraint::Min(20),
        Constraint::Length(12),
        Constraint::Length(22),
    ];

    let title = match &app.selected_pipeline_name {
        Some(name) => format!(" Pipeline Executions — {} ", name),
        None => " Pipeline Executions ".to_string(),
    };

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Cyan)),
        );

    f.render_widget(table, area);
}
