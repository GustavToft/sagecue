use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::model::step::StepType;

const PALETTE: [Color; 8] = [
    Color::Cyan,
    Color::Magenta,
    Color::Yellow,
    Color::Green,
    Color::Red,
    Color::Blue,
    Color::LightCyan,
    Color::LightMagenta,
];

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let step_name = app.selected_step_name().unwrap_or_default().to_string();

    let step = app.steps.get(app.selected_step);
    let is_training = step
        .map(|s| matches!(s.step_type, StepType::Training))
        .unwrap_or(false);

    if !is_training {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Metrics: {} ", step_name))
            .border_style(Style::default().fg(Color::Magenta));
        let para = Paragraph::new("Metrics only available for Training steps")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(para, area);
        return;
    }

    let step_metrics = app.metrics_state.metrics_for_step(&step_name);

    let has_final = step_metrics
        .map(|m| !m.final_metrics.is_empty())
        .unwrap_or(false);
    let has_series = step_metrics
        .map(|m| m.experiment_series.iter().any(|s| !s.points.is_empty()))
        .unwrap_or(false);

    if !has_final && !has_series {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Metrics: {} ", step_name))
            .border_style(Style::default().fg(Color::Magenta));
        let para = Paragraph::new("Waiting for metrics...")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(para, area);
        return;
    }

    let step_metrics = step_metrics.unwrap();

    // Collect non-empty series names for selector, sorted alphabetically
    let mut series_names: Vec<String> = step_metrics
        .experiment_series
        .iter()
        .filter(|s| !s.points.is_empty())
        .map(|s| s.metric_name.clone())
        .collect();
    series_names.sort();

    // Ensure defaults (check all if none checked)
    app.metrics_state.ensure_defaults(&series_names);

    // Refetch after mutation
    let step_metrics = app.metrics_state.metrics_for_step(&step_name).unwrap();

    if has_series {
        // Layout: selector (1/4) | chart (3/4)
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Ratio(1, 4), Constraint::Ratio(3, 4)])
            .split(area);

        // Left panel: selector + optional final metrics
        if has_final {
            let final_height = (step_metrics.final_metrics.len() as u16 + 2)
                .min(chunks[0].height / 2);
            let left_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(final_height)])
                .split(chunks[0]);
            draw_selector(f, app, &step_name, &series_names, left_chunks[0]);
            draw_final_metrics(f, &step_name, step_metrics, left_chunks[1]);
        } else {
            draw_selector(f, app, &step_name, &series_names, chunks[0]);
        }

        draw_chart(f, app, &step_name, &series_names, chunks[1]);
    } else {
        draw_final_metrics(f, &step_name, step_metrics, area);
    }
}

fn draw_selector(
    f: &mut Frame,
    app: &App,
    _step_name: &str,
    series_names: &[String],
    area: Rect,
) {
    let step_name = app.selected_step_name().unwrap_or_default();
    let step_metrics = app.metrics_state.metrics_for_step(step_name);
    let cursor = app.metrics_state.metrics_cursor;

    let mut lines: Vec<Line> = Vec::new();

    for (i, name) in series_names.iter().enumerate() {
        let checked = app.metrics_state.metrics_checked.contains(name);
        let color = PALETTE[i % PALETTE.len()];
        let is_cursor = i == cursor;

        let prefix = if is_cursor { ">" } else { " " };
        let check = if checked { "x" } else { " " };

        // Find latest value
        let latest_val = step_metrics
            .and_then(|m| m.experiment_series.iter().find(|s| s.metric_name == *name))
            .and_then(|s| s.points.last())
            .map(|(_, v)| format!("{:.4}", v))
            .unwrap_or_default();

        let style = if is_cursor {
            Style::default().bg(Color::DarkGray)
        } else {
            Style::default()
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{}[{}] ", prefix, check), style.fg(color)),
            Span::styled(
                format!("{:<20}", truncate_name(name, 20)),
                style.fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(latest_val, style.fg(Color::Yellow)),
        ]));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Metrics ")
        .border_style(Style::default().fg(Color::Magenta));

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, area);
}

fn draw_chart(
    f: &mut Frame,
    app: &App,
    step_name: &str,
    series_names: &[String],
    area: Rect,
) {
    let step_metrics = app.metrics_state.metrics_for_step(step_name);

    // Build data for checked series
    let checked_series: Vec<_> = series_names
        .iter()
        .enumerate()
        .filter(|(_, name)| app.metrics_state.metrics_checked.contains(name.as_str()))
        .filter_map(|(i, name)| {
            step_metrics
                .and_then(|m| m.experiment_series.iter().find(|s| s.metric_name == *name))
                .map(|s| {
                    let points: Vec<(f64, f64)> =
                        s.points.iter().map(|&(x, y)| (x as f64, y)).collect();
                    (i, name.as_str(), points)
                })
        })
        .collect();

    if checked_series.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Chart ")
            .border_style(Style::default().fg(Color::Magenta));
        let para = Paragraph::new("No metrics selected (Space to toggle, a for all)")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(para, area);
        return;
    }

    // Compute axis bounds from visible data
    let mut x_min = f64::MAX;
    let mut x_max = f64::MIN;
    let mut y_min = f64::MAX;
    let mut y_max = f64::MIN;

    for (_, _, points) in &checked_series {
        for &(x, y) in points {
            if x < x_min { x_min = x; }
            if x > x_max { x_max = x; }
            if y < y_min { y_min = y; }
            if y > y_max { y_max = y; }
        }
    }

    // Handle edge cases
    if x_min == x_max {
        x_min -= 1.0;
        x_max += 1.0;
    }
    let y_margin = (y_max - y_min).abs() * 0.05;
    if y_margin == 0.0 {
        y_min -= 1.0;
        y_max += 1.0;
    } else {
        y_min -= y_margin;
        y_max += y_margin;
    }

    // Build datasets — we need to hold the data in a Vec that outlives the datasets
    let datasets: Vec<Dataset> = checked_series
        .iter()
        .map(|(i, name, points)| {
            let color = PALETTE[*i % PALETTE.len()];
            Dataset::default()
                .name(truncate_name(name, 15).to_string())
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(color))
                .data(points)
        })
        .collect();

    let x_labels = vec![
        Span::raw(format!("{:.0}", x_min)),
        Span::raw(format!("{:.0}", (x_min + x_max) / 2.0)),
        Span::raw(format!("{:.0}", x_max)),
    ];
    let y_labels = vec![
        Span::raw(format!("{:.4}", y_min)),
        Span::raw(format!("{:.4}", (y_min + y_max) / 2.0)),
        Span::raw(format!("{:.4}", y_max)),
    ];

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Chart ")
                .border_style(Style::default().fg(Color::Magenta)),
        )
        .x_axis(
            Axis::default()
                .title("Epoch")
                .style(Style::default().fg(Color::Gray))
                .bounds([x_min, x_max])
                .labels(x_labels),
        )
        .y_axis(
            Axis::default()
                .title("Value")
                .style(Style::default().fg(Color::Gray))
                .bounds([y_min, y_max])
                .labels(y_labels),
        );

    f.render_widget(chart, area);
}

fn draw_final_metrics(
    f: &mut Frame,
    step_name: &str,
    metrics: &crate::model::metrics::StepMetrics,
    area: Rect,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Final: {} ", step_name))
        .border_style(Style::default().fg(Color::Green));

    let lines: Vec<Line> = metrics
        .final_metrics
        .iter()
        .map(|m| {
            let ts = m.timestamp.format("%H:%M:%S").to_string();
            Line::from(vec![
                Span::styled(
                    format!("  {:<30}", m.metric_name),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("{:>12.6}", m.value),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    format!("  {}", ts),
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        })
        .collect();

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, area);
}

fn truncate_name(name: &str, max_len: usize) -> &str {
    if name.len() <= max_len {
        name
    } else {
        &name[..max_len]
    }
}
