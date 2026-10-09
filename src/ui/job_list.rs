use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use ratatui::Frame;

use super::execution_list::status_color;
use crate::app::App;
use crate::model::format::fmt_local;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Jobs ")
        .border_style(Style::default().fg(Color::Cyan));

    if app.loading && app.jobs.is_empty() {
        let loading = ratatui::widgets::Paragraph::new("Loading jobs...")
            .style(Style::default().fg(Color::Yellow))
            .block(block);
        f.render_widget(loading, area);
        return;
    }

    if app.jobs.is_empty() {
        let msg = if let Some(ref err) = app.error_message {
            format!("Error: {}", err)
        } else {
            "No jobs found".to_string()
        };
        let para = ratatui::widgets::Paragraph::new(msg)
            .style(Style::default().fg(Color::Red))
            .block(block);
        f.render_widget(para, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from("  "),
        Cell::from("Job"),
        Cell::from("Type"),
        Cell::from("Status"),
        Cell::from("Created"),
        Cell::from("Duration"),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = app
        .jobs
        .iter()
        .enumerate()
        .map(|(i, job)| {
            let is_selected = i == app.job_cursor;
            let prefix = if is_selected { "> " } else { "  " };
            let created = job
                .created
                .map(|t| fmt_local(t, "%Y-%m-%d %H:%M:%S"))
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
                Cell::from(job.name.clone()),
                Cell::from(job.job_type.as_str()),
                Cell::from(job.status.as_str().to_string())
                    .style(Style::default().fg(status_color(&job.status))),
                Cell::from(created),
                Cell::from(job.duration_str()),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(2),
        Constraint::Min(20),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(22),
        Constraint::Length(12),
    ];

    let table = Table::new(rows, widths).header(header).block(block);

    f.render_widget(table, area);
}
