use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use ratatui::Frame;
use ratatui::layout::Constraint;

use crate::app::App;
use crate::model::step::StepStatus;

fn status_icon(status: &StepStatus) -> &'static str {
    match status {
        StepStatus::NotStarted => "·",
        StepStatus::Executing => "●",
        StepStatus::Succeeded => "✓",
        StepStatus::Failed => "✗",
        StepStatus::Stopped => "■",
        StepStatus::Unknown(_) => "?",
    }
}

fn status_color(status: &StepStatus) -> Color {
    match status {
        StepStatus::NotStarted => Color::DarkGray,
        StepStatus::Executing => Color::Yellow,
        StepStatus::Succeeded => Color::Green,
        StepStatus::Failed => Color::Red,
        StepStatus::Stopped => Color::Red,
        StepStatus::Unknown(_) => Color::Gray,
    }
}

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let header = Row::new(vec![
        Cell::from(" # "),
        Cell::from("Step"),
        Cell::from("Type"),
        Cell::from("Status"),
        Cell::from("Start"),
        Cell::from("Duration"),
        Cell::from("Details"),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let widths = [
        Constraint::Length(3),
        Constraint::Length(28),
        Constraint::Length(13),
        Constraint::Length(16),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Min(10),
    ];

    if app.steps.is_empty() {
        let empty_row = Row::new(vec![
            Cell::from(""),
            Cell::from("Loading...").style(Style::default().fg(Color::DarkGray)),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
        ]);

        let table = Table::new(vec![empty_row], widths)
            .header(header)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Steps ")
                    .border_style(Style::default().fg(Color::Cyan)),
            );

        f.render_widget(table, area);
        return;
    }

    let rows: Vec<Row> = app
        .steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let color = status_color(&step.status);
            let icon = status_icon(&step.status);
            let is_selected = i == app.selected_step;

            let status_text = format!("{} {}", icon, step.status.as_str());

            let style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(color)
            };

            let prefix = if is_selected { ">" } else { " " };

            Row::new(vec![
                Cell::from(format!("{}{}", prefix, i + 1)),
                Cell::from(step.name.clone()),
                Cell::from(step.step_type.as_str()),
                Cell::from(status_text).style(Style::default().fg(color)),
                Cell::from(step.start_time_str()),
                Cell::from(step.duration_str()),
                Cell::from(step.detail_str()),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Steps ")
                .border_style(Style::default().fg(Color::Cyan)),
        );

    f.render_widget(table, area);
}
