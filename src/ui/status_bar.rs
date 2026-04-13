use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::MonitorTab;

fn key_span(key: &str, desc: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            format!(" {} ", key),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {} ", desc), Style::default().fg(Color::Gray)),
    ]
}

pub fn draw_monitor_bar(
    f: &mut Frame,
    area: Rect,
    notifications_enabled: bool,
    watcher_count: usize,
    is_executing: bool,
    active_tab: MonitorTab,
) {
    let mut spans: Vec<Span> = Vec::new();
    spans.extend(key_span("Esc", "Back"));
    spans.extend(key_span("q", "Quit"));
    spans.extend(key_span("↑↓", "Step"));
    spans.extend(key_span("Tab", "Switch"));
    let (logs_style, metrics_style) = match active_tab {
        MonitorTab::Logs => (
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(Color::Rgb(80, 80, 80)),
        ),
        MonitorTab::Metrics => (
            Style::default().fg(Color::Rgb(80, 80, 80)),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
    };
    spans.push(Span::styled("[Logs]", logs_style));
    spans.push(Span::styled("[Metrics] ", metrics_style));
    match active_tab {
        MonitorTab::Logs => {
            spans.extend(key_span("j/k", "Scroll"));
            spans.extend(key_span("G/g", "End/Start"));
        }
        MonitorTab::Metrics => {
            spans.extend(key_span("j/k", "Select"));
            spans.extend(key_span("Space", "Toggle"));
            spans.extend(key_span("a", "All"));
        }
    }
    if is_executing {
        spans.extend(key_span("S", "Stop"));
    }
    spans.extend(key_span("R", "Restart"));
    spans.extend(key_span("n", "Notify"));
    if notifications_enabled {
        spans.push(Span::styled(
            "[ON] ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
    } else {
        spans.push(Span::styled(
            "[OFF] ",
            Style::default().fg(Color::Rgb(80, 80, 80)),
        ));
    }
    if watcher_count > 0 {
        spans.push(Span::styled(
            format!("Watch: {} ", watcher_count),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let line = Line::from(spans);
    let bar = Paragraph::new(line).style(Style::default().bg(Color::DarkGray));
    f.render_widget(bar, area);
}

pub fn draw_pipeline_list_bar(f: &mut Frame, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();
    spans.extend(key_span("q", "Quit"));
    spans.extend(key_span("↑↓", "Select"));
    spans.extend(key_span("Enter", "Executions"));

    let line = Line::from(spans);
    let bar = Paragraph::new(line).style(Style::default().bg(Color::DarkGray));
    f.render_widget(bar, area);
}

pub fn draw_execution_list_bar(f: &mut Frame, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();
    spans.extend(key_span("Esc", "Back"));
    spans.extend(key_span("q", "Quit"));
    spans.extend(key_span("↑↓", "Select"));
    spans.extend(key_span("Enter", "Monitor"));

    let line = Line::from(spans);
    let bar = Paragraph::new(line).style(Style::default().bg(Color::DarkGray));
    f.render_widget(bar, area);
}
