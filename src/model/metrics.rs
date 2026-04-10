use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};

/// A single metric value with timestamp (from DescribeTrainingJob's final_metric_data_list).
#[derive(Debug, Clone)]
pub struct MetricDataPoint {
    pub metric_name: String,
    pub timestamp: DateTime<Utc>,
    pub value: f64,
}

/// A time-series for one metric from SageMaker Experiments (batch_get_metrics).
/// X-axis is the step/epoch number, not a timestamp.
#[derive(Debug, Clone)]
pub struct ExperimentTimeSeries {
    pub metric_name: String,
    /// (step/epoch, value) pairs sorted by step.
    pub points: Vec<(i64, f64)>,
}

/// Combined metrics data for a step.
#[derive(Debug, Clone, Default)]
pub struct StepMetrics {
    /// Latest final metrics from DescribeTrainingJob.
    pub final_metrics: Vec<MetricDataPoint>,
    /// Time-series from SageMaker Experiments (batch_get_metrics).
    pub experiment_series: Vec<ExperimentTimeSeries>,
}

#[derive(Debug)]
pub struct MetricsState {
    pub per_step_cache: HashMap<String, StepMetrics>,
    pub metrics_cursor: usize,
    pub metrics_checked: HashSet<String>,
}

impl MetricsState {
    pub fn new() -> Self {
        Self {
            per_step_cache: HashMap::new(),
            metrics_cursor: 0,
            metrics_checked: HashSet::new(),
        }
    }

    pub fn metrics_for_step(&self, step_name: &str) -> Option<&StepMetrics> {
        self.per_step_cache.get(step_name)
    }

    pub fn toggle_metric(&mut self, name: &str) {
        if !self.metrics_checked.remove(name) {
            self.metrics_checked.insert(name.to_string());
        }
    }

    pub fn toggle_all(&mut self, all_names: &[String]) {
        if all_names.iter().all(|n| self.metrics_checked.contains(n)) {
            self.metrics_checked.clear();
        } else {
            for n in all_names {
                self.metrics_checked.insert(n.clone());
            }
        }
    }

    pub fn cursor_up(&mut self) {
        self.metrics_cursor = self.metrics_cursor.saturating_sub(1);
    }

    pub fn cursor_down(&mut self, max: usize) {
        if max > 0 && self.metrics_cursor < max - 1 {
            self.metrics_cursor += 1;
        }
    }

    pub fn reset_selection(&mut self) {
        self.metrics_cursor = 0;
        self.metrics_checked.clear();
    }

    /// If no metrics are checked and series data exists, check all by default.
    pub fn ensure_defaults(&mut self, series_names: &[String]) {
        if self.metrics_checked.is_empty() && !series_names.is_empty() {
            for name in series_names {
                self.metrics_checked.insert(name.clone());
            }
        }
    }
}
