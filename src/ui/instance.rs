use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Sparkline};
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

fn title_for(step_name: &str, util: Option<&UtilizationMetrics>) -> String {
    let mut title = format!(" Instance: {} ", step_name);
    if let Some(u) = util {
        if let Some(ref t) = u.instance_type {
            title = format!(" Instance: {} — {} ", step_name, t);
        }
        if let Some(n) = u.instance_count {
            if n > 1 {
                title.pop(); // remove trailing space
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

    // Each row: 1 line label + remaining sparkline. Distribute rows evenly.
    let constraints: Vec<Constraint> = visible
        .iter()
        .map(|_| Constraint::Ratio(1, visible.len() as u32))
        .collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (series, row) in visible.iter().zip(rows.iter()) {
        draw_row(f, series, *row);
    }
}

fn draw_row(f: &mut Frame, series: &UtilizationSeries, area: Rect) {
    if area.height == 0 {
        return;
    }
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    let latest = series.points.last().map(|(_, v)| *v).unwrap_or(0.0);
    let suffix = if is_summed(&series.metric_name) {
        " (summed)"
    } else {
        ""
    };
    let label = format!(
        "{:<11} {:>6.1}%{}",
        short_label(&series.metric_name),
        latest,
        suffix
    );
    let label_para = Paragraph::new(label).style(
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );
    f.render_widget(label_para, split[0]);

    let data: Vec<u64> = series
        .points
        .iter()
        .map(|(_, v)| v.max(0.0).round() as u64)
        .collect();
    let max = data.iter().copied().max().unwrap_or(1).max(1);

    let color = match series.metric_name.as_str() {
        "CPUUtilization" => Color::Cyan,
        "MemoryUtilization" => Color::Magenta,
        "DiskUtilization" => Color::Yellow,
        "GPUUtilization" => Color::LightGreen,
        "GPUMemoryUtilization" => Color::LightMagenta,
        _ => Color::White,
    };
    let spark = Sparkline::default()
        .data(&data)
        .max(max)
        .style(Style::default().fg(color));
    f.render_widget(spark, split[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
