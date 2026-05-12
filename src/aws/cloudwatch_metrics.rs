use std::time::Duration;

use anyhow::{Context, Result};
use aws_sdk_cloudwatch::primitives::DateTime as AwsDateTime;
use aws_sdk_cloudwatch::types::{Dimension, Metric, MetricDataQuery, MetricStat, ScanBy};
use aws_sdk_cloudwatch::Client as CloudWatchClient;
use chrono::{DateTime, TimeZone, Utc};

use crate::model::metrics::{UtilizationMetrics, UtilizationSeries};
use crate::model::step::{is_gpu_instance, JobType};

/// Cap on `get_metric_data` pagination iterations. At 60s period the API
/// returns up to 1440 datapoints per call (~24h), so 10 pages covers ~10 days
/// — far more than any realistic SageMaker job.
const MAX_PAGES: usize = 10;
/// Padding around the step's start/end so we catch the first datapoint that
/// CloudWatch publishes ~1 min after the job starts and any tail metrics.
const PADDING: Duration = Duration::from_secs(120);

/// CloudWatch metric names we ask for, in the fixed order the UI renders them.
const ALWAYS_METRICS: &[(&str, &str)] = &[
    ("cpu", "CPUUtilization"),
    ("mem", "MemoryUtilization"),
    ("disk", "DiskUtilization"),
];
const GPU_METRICS: &[(&str, &str)] = &[
    ("gpu", "GPUUtilization"),
    ("gpumem", "GPUMemoryUtilization"),
];

fn namespace_for(job_type: &JobType) -> &'static str {
    match job_type {
        JobType::Training => "/aws/sagemaker/TrainingJobs",
        JobType::Processing => "/aws/sagemaker/ProcessingJobs",
        JobType::Transform => "/aws/sagemaker/TransformJobs",
    }
}

/// Inputs to `fetch_instance_utilization`. Bundled into a struct to keep the
/// call site readable and stay under the `clippy::too_many_arguments` limit.
pub struct UtilizationRequest<'a> {
    pub job_type: JobType,
    pub job_name: &'a str,
    pub instance_type: Option<&'a str>,
    pub instance_count: Option<i32>,
    pub step_start: Option<DateTime<Utc>>,
    pub step_end: Option<DateTime<Utc>>,
    pub fallback_window: Duration,
}

/// Fetch host-level CloudWatch utilization for one SageMaker job.
///
/// The query window is anchored to the step's `start_time`/`end_time` when
/// available so the user sees the whole job timeline, not just the last
/// few minutes. For an in-progress step (no `end_time`) we extend to `now`.
/// When neither timestamp is known we fall back to `fallback_window` ending
/// at `now`.
///
/// Queries algo-1 only — multi-host fan-out is intentionally out of scope for
/// v1. On a GPU instance type (`ml.p*` / `ml.g*`) the GPU and GPU-memory
/// series are included; otherwise they're omitted entirely. Paginates through
/// `NextToken` so long-running jobs return their full timeline.
pub async fn fetch_instance_utilization(
    client: &CloudWatchClient,
    req: UtilizationRequest<'_>,
) -> Result<UtilizationMetrics> {
    let namespace = namespace_for(&req.job_type);
    let host = format!("{}/algo-1", req.job_name);
    let wants_gpu = req.instance_type.map(is_gpu_instance).unwrap_or(false);

    let mut planned: Vec<(&str, &str)> = ALWAYS_METRICS.to_vec();
    if wants_gpu {
        planned.extend_from_slice(GPU_METRICS);
    }

    let (start_dt, end_dt) = compute_window(
        req.step_start,
        req.step_end,
        req.fallback_window,
        Utc::now(),
    );

    let mut queries: Vec<MetricDataQuery> = Vec::with_capacity(planned.len());
    for (id, metric_name) in &planned {
        let dim = Dimension::builder().name("Host").value(&host).build();
        let metric = Metric::builder()
            .namespace(namespace)
            .metric_name(*metric_name)
            .dimensions(dim)
            .build();
        let stat = MetricStat::builder()
            .metric(metric)
            .period(60)
            .stat("Average")
            .build();
        let query = MetricDataQuery::builder()
            .id(*id)
            .label(*metric_name)
            .metric_stat(stat)
            .return_data(true)
            .build();
        queries.push(query);
    }

    tracing::debug!(
        namespace = namespace,
        host = %host,
        metric_count = queries.len(),
        start = %start_dt,
        end = %end_dt,
        "get_metric_data request"
    );

    let mut accum: std::collections::HashMap<String, Vec<(DateTime<Utc>, f64)>> =
        std::collections::HashMap::new();
    let mut next_token: Option<String> = None;
    let start_aws = AwsDateTime::from_secs(start_dt.timestamp());
    let end_aws = AwsDateTime::from_secs(end_dt.timestamp());

    for page in 0..MAX_PAGES {
        let mut req = client
            .get_metric_data()
            .set_metric_data_queries(Some(queries.clone()))
            .start_time(start_aws)
            .end_time(end_aws)
            .scan_by(ScanBy::TimestampAscending);
        if let Some(t) = next_token.as_ref() {
            req = req.next_token(t);
        }

        let resp = req.send().await.context("Failed to get_metric_data")?;

        for r in resp.metric_data_results() {
            let id = r.id().unwrap_or_default().to_string();
            let bucket = accum.entry(id).or_default();
            for (t, v) in r.timestamps().iter().zip(r.values().iter()) {
                if let Some(dt) = smithy_to_chrono(t) {
                    bucket.push((dt, *v));
                }
            }
        }

        match resp.next_token() {
            Some(t) if !t.is_empty() => {
                next_token = Some(t.to_string());
                tracing::debug!(page = page + 1, "get_metric_data: paginating");
            }
            _ => break,
        }
    }

    let parsed: Vec<ParsedResult> = accum
        .into_iter()
        .map(|(id, mut points)| {
            // CloudWatch returns each page sorted by timestamp; ensure overall order.
            points.sort_by_key(|(t, _)| *t);
            tracing::debug!(id = %id, point_count = points.len(), "get_metric_data result");
            ParsedResult { id, points }
        })
        .collect();

    Ok(assemble_metrics(
        &planned,
        parsed,
        req.instance_type.map(|s| s.to_string()),
        req.instance_count,
    ))
}

/// Pick the CloudWatch query window from the step lifecycle.
fn compute_window(
    step_start: Option<DateTime<Utc>>,
    step_end: Option<DateTime<Utc>>,
    fallback_window: Duration,
    now: DateTime<Utc>,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let pad =
        chrono::Duration::from_std(PADDING).unwrap_or_else(|_| chrono::Duration::seconds(120));
    let fallback = chrono::Duration::from_std(fallback_window)
        .unwrap_or_else(|_| chrono::Duration::seconds(900));

    let start = step_start.map(|s| s - pad).unwrap_or(now - fallback);
    let end = step_end.map(|e| e + pad).unwrap_or(now);
    // Clamp end to now — CloudWatch rejects future end times.
    let end = end.min(now);
    // Sanity: ensure start < end.
    let start = start.min(end - chrono::Duration::seconds(60));
    (start, end)
}

fn smithy_to_chrono(dt: &AwsDateTime) -> Option<DateTime<Utc>> {
    let secs = dt.secs();
    let nanos = dt.subsec_nanos();
    Utc.timestamp_opt(secs, nanos).single()
}

/// Pure helper that turns the parsed `get_metric_data` response back into
/// `UtilizationMetrics`, preserving the query order and dropping empty series.
/// Split out so it can be unit-tested without the SDK.
fn assemble_metrics(
    planned: &[(&str, &str)],
    mut results: Vec<ParsedResult>,
    instance_type: Option<String>,
    instance_count: Option<i32>,
) -> UtilizationMetrics {
    let mut series: Vec<UtilizationSeries> = Vec::new();
    for (id, metric_name) in planned {
        if let Some(pos) = results.iter().position(|r| r.id == *id) {
            let r = results.swap_remove(pos);
            if r.points.is_empty() {
                continue;
            }
            series.push(UtilizationSeries {
                metric_name: (*metric_name).to_string(),
                points: r.points,
            });
        }
    }
    UtilizationMetrics {
        series,
        instance_type,
        instance_count,
    }
}

#[derive(Debug, Clone)]
struct ParsedResult {
    id: String,
    points: Vec<(DateTime<Utc>, f64)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).single().unwrap()
    }

    fn planned_cpu_only() -> Vec<(&'static str, &'static str)> {
        vec![
            ("cpu", "CPUUtilization"),
            ("mem", "MemoryUtilization"),
            ("disk", "DiskUtilization"),
        ]
    }

    fn planned_gpu() -> Vec<(&'static str, &'static str)> {
        let mut v = planned_cpu_only();
        v.push(("gpu", "GPUUtilization"));
        v.push(("gpumem", "GPUMemoryUtilization"));
        v
    }

    #[test]
    fn assemble_cpu_instance_yields_three_series_in_order() {
        let planned = planned_cpu_only();
        let results = vec![
            ParsedResult {
                id: "disk".to_string(),
                points: vec![(ts(30), 5.0)],
            },
            ParsedResult {
                id: "cpu".to_string(),
                points: vec![(ts(0), 12.0), (ts(60), 80.0)],
            },
            ParsedResult {
                id: "mem".to_string(),
                points: vec![(ts(0), 40.0)],
            },
        ];

        let out = assemble_metrics(&planned, results, Some("ml.m5.large".into()), Some(1));

        let names: Vec<&str> = out.series.iter().map(|s| s.metric_name.as_str()).collect();
        assert_eq!(
            names,
            vec!["CPUUtilization", "MemoryUtilization", "DiskUtilization"]
        );
        assert_eq!(out.instance_type.as_deref(), Some("ml.m5.large"));
        assert_eq!(out.instance_count, Some(1));
    }

    #[test]
    fn assemble_gpu_instance_yields_five_series_in_order() {
        let planned = planned_gpu();
        let results = vec![
            ParsedResult {
                id: "cpu".into(),
                points: vec![(ts(0), 100.0)],
            },
            ParsedResult {
                id: "mem".into(),
                points: vec![(ts(0), 50.0)],
            },
            ParsedResult {
                id: "disk".into(),
                points: vec![(ts(0), 5.0)],
            },
            ParsedResult {
                id: "gpu".into(),
                points: vec![(ts(0), 80.0)],
            },
            ParsedResult {
                id: "gpumem".into(),
                points: vec![(ts(0), 60.0)],
            },
        ];

        let out = assemble_metrics(&planned, results, None, None);

        let names: Vec<&str> = out.series.iter().map(|s| s.metric_name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "CPUUtilization",
                "MemoryUtilization",
                "DiskUtilization",
                "GPUUtilization",
                "GPUMemoryUtilization",
            ]
        );
    }

    #[test]
    fn assemble_drops_empty_series() {
        let planned = planned_cpu_only();
        let results = vec![
            ParsedResult {
                id: "cpu".into(),
                points: vec![(ts(0), 10.0)],
            },
            ParsedResult {
                id: "mem".into(),
                points: vec![],
            },
            ParsedResult {
                id: "disk".into(),
                points: vec![(ts(0), 1.0)],
            },
        ];

        let out = assemble_metrics(&planned, results, None, None);

        let names: Vec<&str> = out.series.iter().map(|s| s.metric_name.as_str()).collect();
        assert_eq!(names, vec!["CPUUtilization", "DiskUtilization"]);
    }

    #[test]
    fn assemble_handles_missing_ids() {
        // The SDK is free to omit results for queries with no data.
        let planned = planned_gpu();
        let results = vec![
            ParsedResult {
                id: "cpu".into(),
                points: vec![(ts(0), 10.0)],
            },
            ParsedResult {
                id: "gpu".into(),
                points: vec![(ts(0), 90.0)],
            },
        ];

        let out = assemble_metrics(&planned, results, None, None);

        let names: Vec<&str> = out.series.iter().map(|s| s.metric_name.as_str()).collect();
        assert_eq!(names, vec!["CPUUtilization", "GPUUtilization"]);
    }

    #[test]
    fn namespace_matches_job_type() {
        assert_eq!(
            namespace_for(&JobType::Training),
            "/aws/sagemaker/TrainingJobs"
        );
        assert_eq!(
            namespace_for(&JobType::Processing),
            "/aws/sagemaker/ProcessingJobs"
        );
        assert_eq!(
            namespace_for(&JobType::Transform),
            "/aws/sagemaker/TransformJobs"
        );
    }

    #[test]
    fn window_uses_job_lifetime_when_known() {
        let now = ts(10_000);
        let start = ts(1_000);
        let end = ts(5_000);
        let (s, e) = compute_window(Some(start), Some(end), Duration::from_secs(900), now);
        // Padded by 120s on each side and end clamped to <= now.
        assert_eq!(s, ts(880));
        assert_eq!(e, ts(5_120));
    }

    #[test]
    fn window_uses_now_when_step_still_running() {
        let now = ts(10_000);
        let start = ts(8_000);
        let (s, e) = compute_window(Some(start), None, Duration::from_secs(900), now);
        assert_eq!(s, ts(7_880));
        assert_eq!(e, now);
    }

    #[test]
    fn window_falls_back_when_no_start_time() {
        let now = ts(10_000);
        let (s, e) = compute_window(None, None, Duration::from_secs(900), now);
        assert_eq!(s, ts(9_100));
        assert_eq!(e, now);
    }

    #[test]
    fn window_clamps_future_end_to_now() {
        // Should never happen in practice but the runtime clock skew could
        // make step_end > now; clamp to now so CloudWatch doesn't reject.
        let now = ts(10_000);
        let start = ts(9_000);
        let end = ts(12_000);
        let (_, e) = compute_window(Some(start), Some(end), Duration::from_secs(900), now);
        assert_eq!(e, now);
    }
}
