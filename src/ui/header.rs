use chrono::Utc;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{stale_level, App, StaleLevel};
use crate::model::execution::ExecutionStatus;
use crate::model::format::{fmt_local, format_duration};

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
        .map(|t| fmt_local(t, "%H:%M:%S"))
        .unwrap_or_else(|| "--".to_string());

    let now = Utc::now();
    // "Updated" means "pipeline last_modified" — real activity on the
    // execution, not our poll cadence.
    let updated_ago = exec
        .last_modified
        .map(|t| format!("{} ago", format_duration((now - t).num_seconds())))
        .unwrap_or_else(|| "--".to_string());

    // Separate marker for poll health — if the poller has stalled (e.g.
    // silent credential expiration or network partition), show "(stale)".
    let stale_marker = match stale_level(app.last_successful_poll, now) {
        StaleLevel::Fresh => None,
        StaleLevel::Stale => Some(" (stale)"),
    };

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
        Line::from({
            let mut spans = vec![
                Span::styled("Created: ", Style::default().fg(Color::Cyan)),
                Span::raw(created_str),
                Span::raw("   "),
                Span::styled("Updated: ", Style::default().fg(Color::Cyan)),
                Span::raw(updated_ago),
            ];
            if let Some(marker) = stale_marker {
                spans.push(Span::styled(
                    marker,
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ));
            }
            spans
        }),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Pipeline Monitor ")
        .border_style(Style::default().fg(Color::Cyan));

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, area);
}
