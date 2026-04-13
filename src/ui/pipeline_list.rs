use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use ratatui::Frame;

use crate::app::App;
use crate::model::format::fmt_local;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    if app.loading && app.pipelines.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Pipelines ")
            .border_style(Style::default().fg(Color::Cyan));
        let loading = ratatui::widgets::Paragraph::new("Loading pipelines...")
            .style(Style::default().fg(Color::Yellow))
            .block(block);
        f.render_widget(loading, area);
        return;
    }

    if app.pipelines.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Pipelines ")
            .border_style(Style::default().fg(Color::Cyan));
        let msg = if let Some(ref err) = app.error_message {
            format!("Error: {}", err)
        } else {
            "No pipelines found".to_string()
        };
        let para = ratatui::widgets::Paragraph::new(msg)
            .style(Style::default().fg(Color::Red))
            .block(block);
        f.render_widget(para, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from("  "),
        Cell::from("Pipeline"),
        Cell::from("Description"),
        Cell::from("Last Execution"),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = app
        .pipelines
        .iter()
        .enumerate()
        .map(|(i, pipeline)| {
            let is_selected = i == app.pipeline_cursor;

            let prefix = if is_selected { "> " } else { "  " };
            let description = pipeline.description.as_deref().unwrap_or("--");
            let last_run = pipeline
                .last_execution_time
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
                Cell::from(pipeline.name.clone()),
                Cell::from(description.to_string()),
                Cell::from(last_run),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(2),
        Constraint::Min(20),
        Constraint::Min(20),
        Constraint::Length(22),
    ];

    let table = Table::new(rows, widths).header(header).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Pipelines ")
            .border_style(Style::default().fg(Color::Cyan)),
    );

    f.render_widget(table, area);
}
