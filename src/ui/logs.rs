use chrono::DateTime;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::model::format::fmt_local;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let step_name = app.selected_step_name().unwrap_or_default();
    let entries = app.log_viewer.entries_for_step(step_name);

    let title = format!(" Logs: {} ", step_name);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));

    if entries.is_empty() {
        let step = app.steps.get(app.selected_step);

        // Show full failure reason in the logs area when there are no logs
        if let Some(reason) = step.and_then(|s| s.failure_reason.as_deref()) {
            let lines: Vec<Line> = vec![
                Line::from(Span::styled(
                    "Failure Reason:",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(reason, Style::default().fg(Color::White))),
            ];
            let para = Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false });
            f.render_widget(para, area);
            return;
        }

        let msg = if step.and_then(|s| s.job_details.as_ref()).is_some() {
            "Waiting for log stream..."
        } else {
            "No logs available (step not started or no job)"
        };
        let para = Paragraph::new(msg)
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(para, area);
        return;
    }

    let inner_height = area.height.saturating_sub(2) as usize; // borders

    // Calculate scroll: if auto_scroll, pin to end
    let scroll_offset = if app.log_viewer.auto_scroll {
        entries.len().saturating_sub(inner_height)
    } else {
        app.log_viewer.scroll_offset.min(entries.len().saturating_sub(inner_height))
    };

    let visible_entries = &entries[scroll_offset..entries.len().min(scroll_offset + inner_height)];

    let lines: Vec<Line> = visible_entries
        .iter()
        .map(|entry| {
            let ts = DateTime::from_timestamp_millis(entry.timestamp)
                .map(|dt| fmt_local(dt, "%H:%M:%S"))
                .unwrap_or_default();

            Line::from(vec![
                Span::styled(
                    format!("[{}] ", ts),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(&entry.message),
            ])
        })
        .collect();

    let scroll_indicator = if entries.len() > inner_height {
        let pct = if entries.is_empty() {
            100
        } else {
            ((scroll_offset + inner_height) * 100) / entries.len()
        };
        format!(" {}% ", pct.min(100))
    } else {
        String::new()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Logs: {} ", step_name))
        .title_bottom(scroll_indicator)
        .border_style(Style::default().fg(Color::Cyan));

    let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    f.render_widget(para, area);
}
