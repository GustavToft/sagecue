use chrono::Utc;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::model::execution::ExecutionStatus;
use crate::model::format::format_duration;

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
    let exec = match &app.execution {
        Some(e) => e,
        None => {
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Pipeline Monitor ");
            let para = Paragraph::new("Loading...").block(block);
            f.render_widget(para, area);
            return;
        }
    };

    let display_name = exec
        .display_name
        .as_deref()
        .unwrap_or("<unnamed>");

    let status_style = Style::default()
        .fg(status_color(&exec.status))
        .add_modifier(Modifier::BOLD);

    let created_str = exec
        .created
        .map(|t| t.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| "--".to_string());

    let updated_ago = exec.last_modified.map(|t| {
        let ago = format_duration((Utc::now() - t).num_seconds());
        format!("{} ago", ago)
    }).unwrap_or_else(|| "--".to_string());

    let lines = vec![
        Line::from(vec![
            Span::styled("Pipeline: ", Style::default().fg(Color::Cyan)),
            Span::raw(app.selected_pipeline_name.as_deref().unwrap_or("<unknown>")),
        ]),
        Line::from(vec![
            Span::styled("Execution: ", Style::default().fg(Color::Cyan)),
            Span::raw(display_name),
            Span::raw("   "),
            Span::styled("Status: ", Style::default().fg(Color::Cyan)),
            Span::styled(exec.status.as_str(), status_style),
        ]),
        Line::from(vec![
            Span::styled("Created: ", Style::default().fg(Color::Cyan)),
            Span::raw(created_str),
            Span::raw("   "),
            Span::styled("Updated: ", Style::default().fg(Color::Cyan)),
            Span::raw(updated_ago),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Pipeline Monitor ")
        .border_style(Style::default().fg(Color::Cyan));

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, area);
}
