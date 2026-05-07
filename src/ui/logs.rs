use chrono::DateTime;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::model::format::fmt_local;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let step_name = app.selected_step_name().unwrap_or_default();
    let entries = app.log_viewer.entries_for_step(step_name);
    let step = app.steps.get(app.selected_step);
    let failure_reason = step.and_then(|s| s.failure_reason.as_deref());

    // Carve off a top strip for the step failure reason whenever one exists.
    // Without this, the reason vanishes the moment CloudWatch returns its
    // first log line, taking the most actionable debugging context with it.
    let (failure_area, logs_area) = match failure_reason {
        Some(reason) if !entries.is_empty() => {
            let height = failure_panel_height(reason, area.width, area.height);
            if height == 0 {
                (None, area)
            } else {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(height), Constraint::Min(3)])
                    .split(area);
                (Some(chunks[0]), chunks[1])
            }
        }
        _ => (None, area),
    };

    if let (Some(area), Some(reason)) = (failure_area, failure_reason) {
        draw_failure_panel(f, area, reason);
    }

    let title = format!(" Logs: {} ", step_name);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));

    if entries.is_empty() {
        // Show full failure reason in the logs area when there are no logs
        if let Some(reason) = failure_reason {
            draw_failure_panel(f, logs_area, reason);
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
        f.render_widget(para, logs_area);
        return;
    }

    let area = logs_area;
    let inner_height = area.height.saturating_sub(2) as usize; // borders

    // Calculate scroll: if auto_scroll, pin to end
    let scroll_offset = if app.log_viewer.auto_scroll {
        entries.len().saturating_sub(inner_height)
    } else {
        app.log_viewer
            .scroll_offset
            .min(entries.len().saturating_sub(inner_height))
    };

    let visible_entries = &entries[scroll_offset..entries.len().min(scroll_offset + inner_height)];

    let lines: Vec<Line> = visible_entries
        .iter()
        .map(|entry| {
            let ts = DateTime::from_timestamp_millis(entry.timestamp)
                .map(|dt| fmt_local(dt, "%H:%M:%S"))
                .unwrap_or_default();

            Line::from(vec![
                Span::styled(format!("[{}] ", ts), Style::default().fg(Color::DarkGray)),
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

    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(para, area);
}

fn draw_failure_panel(f: &mut Frame, area: Rect, reason: &str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Failure Reason ")
        .border_style(Style::default().fg(Color::Red));
    let lines = vec![Line::from(Span::styled(
        reason,
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    ))];
    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(para, area);
}

/// Compute how many rows to reserve at the top of the logs area for the
/// step's failure reason. Returns `0` when the area is too small to safely
/// split (caller should then skip the strip and render logs full-bleed).
fn failure_panel_height(reason: &str, area_width: u16, area_height: u16) -> u16 {
    // 2 borders. Body wraps inside (width - 2).
    let inner_width = area_width.saturating_sub(2).max(1) as usize;
    let body_lines = reason.chars().count().div_ceil(inner_width).max(1) as u16;
    let desired = 2 + body_lines;

    // Leave at least 5 rows for logs (title + a few entries). If the area
    // can't accommodate that plus the smallest useful failure panel (3 rows:
    // 2 borders + 1 line), don't split — let the caller fall back.
    let reserved_for_logs: u16 = 5;
    if area_height < reserved_for_logs + 3 {
        return 0;
    }
    let max_allowed = area_height - reserved_for_logs;
    desired.min(max_allowed).min(10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_reason_uses_minimum_height() {
        // "boom" → 1 body line + 2 borders = 3 rows
        assert_eq!(failure_panel_height("boom", 80, 30), 3);
    }

    #[test]
    fn long_reason_caps_at_ten_rows() {
        let reason = "a".repeat(1000);
        assert_eq!(failure_panel_height(&reason, 80, 40), 10);
    }

    #[test]
    fn small_area_returns_zero() {
        // area_height too small for both a failure strip and logs.
        assert_eq!(failure_panel_height("boom", 80, 7), 0);
    }

    #[test]
    fn medium_reason_scales_with_lines() {
        // 200 chars at inner width 78 → 3 wrapped lines + 2 borders = 5 rows
        let reason = "a".repeat(200);
        assert_eq!(failure_panel_height(&reason, 80, 30), 5);
    }

    #[test]
    fn respects_logs_minimum() {
        // area_height=10, reserved=5, max_allowed=5. A 6-line reason wants
        // 8 rows but should clamp to 5 to keep logs at 5.
        let reason = "a".repeat(500);
        assert_eq!(failure_panel_height(&reason, 80, 10), 5);
    }
}
