use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::model::metrics::{UtilizationMetrics, UtilizationSeries};

/// Metrics published as a sum across cores/GPUs rather than 0-100.
fn is_summed(metric: &str) -> bool {
    matches!(metric, "CPUUtilization" | "GPUUtilization")
}

/// Pretty label for the metric row.
fn short_label(metric: &str) -> &'static str {
    match metric {
        "CPUUtilization" => "CPU",
        "MemoryUtilization" => "Memory",
        "DiskUtilization" => "Disk",
        "GPUUtilization" => "GPU",
        "GPUMemoryUtilization" => "GPU Memory",
        _ => "?",
    }
}

fn color_for(metric: &str) -> Color {
    match metric {
        "CPUUtilization" => Color::Cyan,
        "MemoryUtilization" => Color::Magenta,
        "DiskUtilization" => Color::Yellow,
        "GPUUtilization" => Color::LightGreen,
        "GPUMemoryUtilization" => Color::LightMagenta,
        _ => Color::White,
    }
}

fn title_for(step_name: &str, util: Option<&UtilizationMetrics>) -> String {
    let mut title = format!(" Instance: {} ", step_name);
    if let Some(u) = util {
        if let Some(ref t) = u.instance_type {
            title = format!(" Instance: {} — {} ", step_name, t);
        }
        if let Some(n) = u.instance_count {
            if n > 1 {
                title.pop();
                title.push_str(&format!("(algo-1 of {}) ", n));
            }
        }
    }
    title
}

fn block(title: String) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Green))
}

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let step_name = app.selected_step_name().unwrap_or_default().to_string();
    let step = app.steps.get(app.selected_step);

    let has_job = step.and_then(|s| s.job_details.as_ref()).is_some();
    if !has_job {
        let para = Paragraph::new("No instance metrics available for this step type")
            .style(Style::default().fg(Color::DarkGray))
            .block(block(title_for(&step_name, None)));
        f.render_widget(para, area);
        return;
    }

    let util = app.utilization_cache.get(&step_name);
    let has_data = util
        .map(|u| u.series.iter().any(|s| !s.points.is_empty()))
        .unwrap_or(false);

    if !has_data {
        let para = Paragraph::new(
            "Waiting for utilization datapoints...\n(first datapoint typically arrives ~1-2 min after the job starts)",
        )
        .style(Style::default().fg(Color::DarkGray))
        .block(block(title_for(&step_name, util)));
        f.render_widget(para, area);
        return;
    }

    let util = util.unwrap();
    let outer = block(title_for(&step_name, Some(util)));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let visible: Vec<&UtilizationSeries> = util
        .series
        .iter()
        .filter(|s| !s.points.is_empty())
        .collect();

    if visible.is_empty() {
        return;
    }

    // Shared X bounds across all series so they line up visually.
    let (x_min, x_max) = shared_x_bounds(&visible);

    let constraints: Vec<Constraint> = visible
        .iter()
        .map(|_| Constraint::Ratio(1, visible.len() as u32))
        .collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (series, row) in visible.iter().zip(rows.iter()) {
        draw_row(f, series, *row, x_min, x_max);
    }
}

fn shared_x_bounds(visible: &[&UtilizationSeries]) -> (f64, f64) {
    let mut x_min = f64::MAX;
    let mut x_max = f64::MIN;
    for s in visible {
        for (t, _) in &s.points {
            let x = t.timestamp() as f64;
            if x < x_min {
                x_min = x;
            }
            if x > x_max {
                x_max = x;
            }
        }
    }
    if x_min == f64::MAX || x_min >= x_max {
        (0.0, 1.0)
    } else {
        (x_min, x_max)
    }
}

fn draw_row(f: &mut Frame, series: &UtilizationSeries, area: Rect, x_min: f64, x_max: f64) {
    if area.height == 0 {
        return;
    }
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    let latest = series.points.last().map(|(_, v)| *v).unwrap_or(0.0);
    let peak = series.points.iter().map(|(_, v)| *v).fold(0.0f64, f64::max);
    let suffix = if is_summed(&series.metric_name) {
        " (summed)"
    } else {
        ""
    };
    let color = color_for(&series.metric_name);

    let header = Line::from(vec![
        Span::styled(
            format!("{:<11}", short_label(&series.metric_name)),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" now {:>6.1}%", latest),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            format!(" peak {:>6.1}%{}", peak, suffix),
            Style::default().fg(Color::Rgb(140, 140, 140)),
        ),
    ]);
    f.render_widget(Paragraph::new(header), split[0]);

    let data: Vec<(f64, f64)> = series
        .points
        .iter()
        .map(|(t, v)| (t.timestamp() as f64, *v))
        .collect();
    let y_max = if peak <= 0.0 { 1.0 } else { peak * 1.05 };

    let dataset = Dataset::default()
        .marker(Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(color))
        .data(&data);

    let chart = Chart::new(vec![dataset])
        .x_axis(
            Axis::default()
                .style(Style::default().fg(Color::Rgb(60, 60, 60)))
                .bounds([x_min, x_max]),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(Color::Rgb(60, 60, 60)))
                .bounds([0.0, y_max]),
        );

    f.render_widget(chart, split[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn is_summed_only_cpu_and_gpu() {
        assert!(is_summed("CPUUtilization"));
        assert!(is_summed("GPUUtilization"));
        assert!(!is_summed("MemoryUtilization"));
        assert!(!is_summed("DiskUtilization"));
        assert!(!is_summed("GPUMemoryUtilization"));
    }

    #[test]
    fn short_label_known_metrics() {
        assert_eq!(short_label("CPUUtilization"), "CPU");
        assert_eq!(short_label("MemoryUtilization"), "Memory");
        assert_eq!(short_label("DiskUtilization"), "Disk");
        assert_eq!(short_label("GPUUtilization"), "GPU");
        assert_eq!(short_label("GPUMemoryUtilization"), "GPU Memory");
    }

    #[test]
    fn title_for_with_instance_type_and_count() {
        let util = UtilizationMetrics {
            series: vec![],
            instance_type: Some("ml.g5.xlarge".to_string()),
            instance_count: Some(4),
        };
        let t = title_for("MyStep", Some(&util));
        assert!(t.contains("MyStep"));
        assert!(t.contains("ml.g5.xlarge"));
        assert!(t.contains("algo-1 of 4"));
    }

    #[test]
    fn title_for_single_instance_no_algo_hint() {
        let util = UtilizationMetrics {
            series: vec![],
            instance_type: Some("ml.m5.large".to_string()),
            instance_count: Some(1),
        };
        let t = title_for("S", Some(&util));
        assert!(!t.contains("algo-1 of"));
        assert!(t.contains("ml.m5.large"));
    }

    fn series(name: &str, points: &[(i64, f64)]) -> UtilizationSeries {
        UtilizationSeries {
            metric_name: name.to_string(),
            points: points
                .iter()
                .map(|&(s, v)| (Utc.timestamp_opt(s, 0).single().unwrap(), v))
                .collect(),
        }
    }

    #[test]
    fn shared_x_bounds_spans_all_series() {
        let a = series("CPUUtilization", &[(100, 10.0), (200, 20.0)]);
        let b = series("MemoryUtilization", &[(150, 5.0), (300, 8.0)]);
        let refs: Vec<&UtilizationSeries> = vec![&a, &b];
        let (x_min, x_max) = shared_x_bounds(&refs);
        assert_eq!(x_min, 100.0);
        assert_eq!(x_max, 300.0);
    }

    #[test]
    fn shared_x_bounds_handles_empty_and_degenerate() {
        let empty: Vec<&UtilizationSeries> = vec![];
        assert_eq!(shared_x_bounds(&empty), (0.0, 1.0));

        let single = series("CPUUtilization", &[(42, 1.0)]);
        let refs: Vec<&UtilizationSeries> = vec![&single];
        // Only one timestamp → degenerate range → fallback.
        assert_eq!(shared_x_bounds(&refs), (0.0, 1.0));
    }
}
