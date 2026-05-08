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

    let display_name = exec.display_name.as_deref().unwrap_or("<unnamed>");

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

    let mut lines = vec![
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

    if !exec.parameters.is_empty() {
        let joined = exec
            .parameters
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(Line::from(vec![
            Span::styled("Params: ", Style::default().fg(Color::Cyan)),
            Span::raw(joined),
        ]));
    }

    if let Some(reason) = &exec.failure_reason {
        let chunks = failure_chunks(reason, area.width);
        let mut iter = chunks.into_iter();
        if let Some(first) = iter.next() {
            lines.push(Line::from(vec![
                Span::styled(
                    FAILURE_PREFIX,
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::raw(first),
            ]));
        }
        for chunk in iter {
            lines.push(Line::from(Span::raw(chunk)));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Pipeline Monitor ")
        .border_style(Style::default().fg(Color::Cyan));

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, area);
}

const FAILURE_PREFIX: &str = "Failure: ";

/// Pre-wrap a pipeline-level failure reason into chunks that fit the header
/// area's inner width. The first chunk leaves room for the "Failure: " prefix
/// span; subsequent chunks use the full inner width. Newlines in the source
/// reason are preserved as line breaks. Returning the chunks (rather than
/// relying on `Paragraph` wrap) lets the layout pre-compute the exact header
/// height in `ui::draw_monitor`.
pub fn failure_chunks(reason: &str, area_width: u16) -> Vec<String> {
    let inner = (area_width.saturating_sub(2) as usize).max(FAILURE_PREFIX.len() + 1);
    let first_max = inner.saturating_sub(FAILURE_PREFIX.len()).max(1);

    let mut out = Vec::new();
    let mut on_first = true;

    for source_line in reason.split('\n') {
        let chars: Vec<char> = source_line.chars().collect();
        if chars.is_empty() {
            out.push(String::new());
            on_first = false;
            continue;
        }
        let mut i = 0;
        while i < chars.len() {
            let max = if on_first { first_max } else { inner };
            on_first = false;
            let end = (i + max).min(chars.len());
            out.push(chars[i..end].iter().collect::<String>());
            i = end;
        }
    }

    if out.is_empty() {
        out.push(String::new());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_reason_one_chunk() {
        let chunks = failure_chunks("boom", 80);
        assert_eq!(chunks, vec!["boom".to_string()]);
    }

    #[test]
    fn first_chunk_accounts_for_prefix() {
        // inner_width = 20 - 2 = 18; first_max = 18 - "Failure: ".len() (9) = 9
        let chunks = failure_chunks("0123456789abcdef", 20);
        assert_eq!(chunks[0], "012345678");
        assert_eq!(chunks[1], "9abcdef");
    }

    #[test]
    fn wraps_long_reason_across_lines() {
        let reason = "a".repeat(100);
        let chunks = failure_chunks(&reason, 30);
        // first line carries prefix → 30 - 2 - 9 = 19 chars
        // subsequent lines use full inner = 28 chars
        assert_eq!(chunks[0].len(), 19);
        assert_eq!(chunks[1].len(), 28);
        let total: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn preserves_explicit_newlines() {
        let chunks = failure_chunks("first\nsecond", 80);
        assert_eq!(chunks, vec!["first".to_string(), "second".to_string()]);
    }

    #[test]
    fn empty_reason_yields_one_blank_line() {
        let chunks = failure_chunks("", 80);
        assert_eq!(chunks, vec![String::new()]);
    }

    #[test]
    fn copes_with_tiny_area_width() {
        // area_width too small to fit prefix → falls back to a 1-char body.
        let chunks = failure_chunks("xyz", 4);
        assert!(!chunks.is_empty());
        let total: usize = chunks.iter().map(|c| c.chars().count()).sum();
        assert_eq!(total, 3);
    }
}
