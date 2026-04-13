use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn draw(f: &mut Frame, app: &App) {
    let Some(editor) = app.parameter_editor.as_ref() else {
        return;
    };

    let area = centered_rect(70, 70, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Start Execution — {} ", editor.pipeline_name))
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // body
            Constraint::Length(1), // error line
            Constraint::Length(1), // footer hint
        ])
        .split(inner);

    if editor.loading {
        let p = Paragraph::new("Loading parameters...").style(Style::default().fg(Color::Yellow));
        f.render_widget(p, chunks[0]);
    } else if editor.parameters.is_empty() {
        let p = Paragraph::new(
            "This pipeline has no parameters.\nPress Enter to start with defaults, Esc to cancel.",
        )
        .style(Style::default().fg(Color::Gray));
        f.render_widget(p, chunks[0]);
    } else {
        let rows: Vec<Line> = editor
            .parameters
            .iter()
            .zip(editor.values.iter())
            .enumerate()
            .map(|(i, (param, value))| {
                let is_selected = i == editor.cursor;
                let required = param.is_required();
                let label = format!("{} [{}]: ", param.name, param.type_name);
                let label_style = if is_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let value_style = if is_selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };
                let marker = if required { "* " } else { "  " };
                let marker_style = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);
                let mut spans = vec![
                    Span::styled(marker.to_string(), marker_style),
                    Span::styled(label, label_style),
                    Span::styled(value.clone(), value_style),
                ];
                if is_selected {
                    spans.push(Span::styled(
                        "│",
                        Style::default().add_modifier(Modifier::SLOW_BLINK),
                    ));
                }
                Line::from(spans)
            })
            .collect();

        let mut body = rows;
        body.push(Line::from(""));
        body.push(Line::from(Span::styled(
            "* required",
            Style::default().fg(Color::Rgb(120, 120, 120)),
        )));

        let p = Paragraph::new(body);
        f.render_widget(p, chunks[0]);
    }

    if let Some(ref err) = editor.error {
        let p = Paragraph::new(format!("Error: {}", err))
            .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
        f.render_widget(p, chunks[1]);
    }

    let hint =
        Paragraph::new("Enter start · Esc cancel · ↑↓ select · Backspace delete · Ctrl+U clear")
            .style(Style::default().fg(Color::Rgb(120, 120, 120)));
    f.render_widget(hint, chunks[2]);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
