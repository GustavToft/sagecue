use anyhow::{Context, Result};
use aws_sdk_sagemaker::Client as SageMakerClient;
use aws_sdk_sagemakermetrics::types::{MetricQuery, MetricStatistic, Period, XAxisType};
use aws_sdk_sagemakermetrics::Client as MetricsClient;
use crate::model::metrics::ExperimentTimeSeries;

/// Look up the trial component ARN for a training job using its job ARN as source.
pub async fn find_trial_component_arn(
    client: &SageMakerClient,
    training_job_arn: &str,
) -> Result<Option<String>> {
    let resp = client
        .list_trial_components()
        .source_arn(training_job_arn)
        .max_results(1)
        .send()
        .await
        .context("Failed to list trial components")?;

    Ok(resp
        .trial_component_summaries()
        .first()
        .and_then(|tc| tc.trial_component_arn().map(|s| s.to_string())))
}

/// Discover all metric names from a trial component.
/// Uses the SDK first, falls back to AWS CLI if SDK parsing fails
/// (known issue with "mixed variants in union" deserialization).
pub async fn discover_metric_names(
    client: &SageMakerClient,
    trial_component_arn: &str,
) -> Result<Vec<String>> {
    let tc_name = trial_component_arn
        .rsplit('/')
        .next()
        .context("Invalid trial component ARN")?;

    // Try SDK first
    match client
        .describe_trial_component()
        .trial_component_name(tc_name)
        .send()
        .await
    {
        Ok(resp) => {
            let names: Vec<String> = resp
                .metrics()
                .iter()
                .filter_map(|m| m.metric_name().map(|n| n.to_string()))
                .collect();
            tracing::info!(
                trial_component = %tc_name,
                metric_count = names.len(),
                "discovered metric names via SDK"
            );
            return Ok(names);
        }
        Err(e) => {
            tracing::warn!(
                error = ?e,
                "SDK describe_trial_component failed, trying AWS CLI fallback"
            );
        }
    }

    // Fallback: use AWS CLI to bypass SDK JSON parsing bug
    discover_metric_names_via_cli(tc_name).await
}

/// Fallback: call `aws sagemaker describe-trial-component` via CLI
/// and parse the JSON to extract metric names.
async fn discover_metric_names_via_cli(tc_name: &str) -> Result<Vec<String>> {
    let output = tokio::process::Command::new("aws")
        .args([
            "sagemaker",
            "describe-trial-component",
            "--trial-component-name",
            tc_name,
        ])
        .output()
        .await
        .context("Failed to run aws CLI")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("aws CLI failed: {}", stderr.trim());
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("Failed to parse CLI JSON")?;

    let mut seen = std::collections::HashSet::new();
    let mut names: Vec<String> = Vec::new();
    if let Some(metrics) = json.get("Metrics").and_then(|m| m.as_array()) {
        for metric in metrics {
            let name = metric.get("MetricName").and_then(|n| n.as_str()).unwrap_or_default();
            let count = metric.get("Count").and_then(|c| c.as_i64()).unwrap_or(0);
            // Only include metrics that have time-series data (Count > 0) and deduplicate
            if count > 0 && !name.is_empty() && seen.insert(name.to_string()) {
                names.push(name.to_string());
            }
        }
    }
    names.sort();

    tracing::info!(
        trial_component = %tc_name,
        metric_count = names.len(),
        metric_names = ?names,
        "discovered metric names via AWS CLI fallback"
    );

    Ok(names)
}

/// Fetch experiment time-series via batch_get_metrics.
/// Uses IterationNumber (epoch) as x-axis.
pub async fn fetch_experiment_metrics(
    client: &MetricsClient,
    trial_component_arn: &str,
    metric_names: &[String],
) -> Result<Vec<ExperimentTimeSeries>> {
    let queries: Vec<MetricQuery> = metric_names
        .iter()
        .map(|name| {
            MetricQuery::builder()
                .metric_name(name)
                .resource_arn(trial_component_arn)
                .metric_stat(MetricStatistic::Last)
                .period(Period::IterationNumber)
                .x_axis_type(XAxisType::IterationNumber)
                .build()
        })
        .collect();

    tracing::debug!(
        trial_component_arn = %trial_component_arn,
        metric_count = queries.len(),
        "batch_get_metrics request"
    );

    let resp = client
        .batch_get_metrics()
        .set_metric_queries(Some(queries))
        .send()
        .await
        .context("Failed to batch_get_metrics")?;

    let mut all_series: Vec<ExperimentTimeSeries> = Vec::new();

    for (i, result) in resp.metric_query_results().iter().enumerate() {
        let metric_name = metric_names.get(i).cloned().unwrap_or_default();
        let x_values = result.x_axis_values();
        let y_values = result.metric_values();

        tracing::debug!(
            metric = %metric_name,
            status = ?result.status(),
            points = x_values.len(),
            "batch_get_metrics result"
        );

        if x_values.is_empty() {
            continue;
        }

        let mut points: Vec<(i64, f64)> = x_values
            .iter()
            .zip(y_values.iter())
            .map(|(&x, &y)| (x, y))
            .collect();

        // Skip pre-training placeholder: if the first point has y=0.0
        // and there are more points, it's likely an initialization value
        // that distorts the chart scale.
        if points.len() > 1 && points[0].1 == 0.0 {
            tracing::debug!(
                metric = %metric_name,
                skipped_point = ?(points[0]),
                "skipping initial zero-value data point"
            );
            points.remove(0);
        }

        tracing::debug!(
            metric = %metric_name,
            first_3 = ?points.iter().take(3).collect::<Vec<_>>(),
            total = points.len(),
            "series data sample"
        );

        all_series.push(ExperimentTimeSeries {
            metric_name,
            points,
        });
    }

    Ok(all_series)
}
