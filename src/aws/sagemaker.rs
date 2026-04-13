use anyhow::{Context, Result};
use aws_sdk_sagemaker::Client;
use aws_sdk_sagemakermetrics::Client as MetricsClient;
use chrono::{DateTime, Utc};

use crate::aws::sagemaker_metrics;
use crate::model::execution::{ExecutionStatus, ExecutionSummary, PipelineExecution};
use crate::model::metrics::{MetricDataPoint, StepMetrics};
use crate::model::pipeline::PipelineSummary;
use crate::model::step::{JobDetails, JobType, StepInfo, StepStatus, StepType};

fn to_chrono(dt: &aws_sdk_sagemaker::primitives::DateTime) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(dt.secs(), dt.subsec_nanos())
}

pub async fn list_pipelines(client: &Client) -> Result<Vec<PipelineSummary>> {
    let resp = client
        .list_pipelines()
        .max_results(50)
        .send()
        .await
        .context("Failed to list pipelines")?;

    let summaries = resp
        .pipeline_summaries()
        .iter()
        .map(|p| PipelineSummary {
            name: p.pipeline_name().unwrap_or_default().to_string(),
            description: p.pipeline_description().map(|s| s.to_string()),
            last_execution_time: p.last_execution_time().and_then(to_chrono),
        })
        .collect();

    Ok(summaries)
}

pub async fn list_executions(
    client: &Client,
    pipeline_name: &str,
    max_results: i32,
) -> Result<Vec<ExecutionSummary>> {
    let resp = client
        .list_pipeline_executions()
        .pipeline_name(pipeline_name)
        .max_results(max_results)
        .send()
        .await
        .context("Failed to list pipeline executions")?;

    let summaries = resp
        .pipeline_execution_summaries()
        .iter()
        .map(|s| ExecutionSummary {
            arn: s.pipeline_execution_arn().unwrap_or_default().to_string(),
            display_name: s.pipeline_execution_display_name().map(|s| s.to_string()),
            status: s
                .pipeline_execution_status()
                .map(|s| s.as_str().parse::<ExecutionStatus>().unwrap())
                .unwrap_or(ExecutionStatus::Unknown("Unknown".to_string())),
            start_time: s.start_time().and_then(to_chrono),
        })
        .collect();

    Ok(summaries)
}

pub async fn describe_execution(client: &Client, execution_arn: &str) -> Result<PipelineExecution> {
    let resp = client
        .describe_pipeline_execution()
        .pipeline_execution_arn(execution_arn)
        .send()
        .await
        .context("Failed to describe pipeline execution")?;

    Ok(PipelineExecution {
        pipeline_arn: resp.pipeline_arn().map(|s| s.to_string()),
        display_name: resp
            .pipeline_execution_display_name()
            .map(|s| s.to_string()),
        status: resp
            .pipeline_execution_status()
            .map(|s| s.as_str().parse::<ExecutionStatus>().unwrap())
            .unwrap_or(ExecutionStatus::Unknown("Unknown".to_string())),
        created: resp.creation_time().and_then(to_chrono),
        last_modified: resp.last_modified_time().and_then(to_chrono),
    })
}

pub async fn stop_pipeline_execution(client: &Client, execution_arn: &str) -> Result<()> {
    let token = uuid::Uuid::new_v4().to_string();
    client
        .stop_pipeline_execution()
        .pipeline_execution_arn(execution_arn)
        .client_request_token(token)
        .send()
        .await
        .context("Failed to stop pipeline execution")?;
    Ok(())
}

pub async fn start_pipeline_execution(client: &Client, pipeline_name: &str) -> Result<String> {
    let token = uuid::Uuid::new_v4().to_string();
    let resp = client
        .start_pipeline_execution()
        .pipeline_name(pipeline_name)
        .client_request_token(token)
        .send()
        .await
        .context("Failed to start pipeline execution")?;

    resp.pipeline_execution_arn()
        .map(|s| s.to_string())
        .context("No execution ARN returned from start_pipeline_execution")
}

/// Extract step type and optional job details from step metadata.
fn extract_step_type_and_job(
    meta: &aws_sdk_sagemaker::types::PipelineExecutionStepMetadata,
) -> (StepType, Option<JobDetails>) {
    if let Some(training) = meta.training_job() {
        let job_details = training.arn().map(|arn| {
            let job_name = arn.rsplit('/').next().unwrap_or_default().to_string();
            JobDetails {
                job_type: JobType::Training,
                job_name,
                job_arn: Some(arn.to_string()),
                secondary_status: None,
                instance_type: None,
                instance_count: None,
            }
        });
        return (StepType::Training, job_details);
    }

    if let Some(processing) = meta.processing_job() {
        let job_details = processing.arn().map(|arn| {
            let job_name = arn.rsplit('/').next().unwrap_or_default().to_string();
            JobDetails {
                job_type: JobType::Processing,
                job_name,
                job_arn: Some(arn.to_string()),
                secondary_status: None,
                instance_type: None,
                instance_count: None,
            }
        });
        return (StepType::Processing, job_details);
    }

    if let Some(transform) = meta.transform_job() {
        let job_details = transform.arn().map(|arn| {
            let job_name = arn.rsplit('/').next().unwrap_or_default().to_string();
            JobDetails {
                job_type: JobType::Transform,
                job_name,
                job_arn: Some(arn.to_string()),
                secondary_status: None,
                instance_type: None,
                instance_count: None,
            }
        });
        return (StepType::Transform, job_details);
    }

    if meta.condition().is_some() {
        return (StepType::Condition, None);
    }

    if meta.register_model().is_some() {
        return (StepType::RegisterModel, None);
    }

    if meta.lambda().is_some() {
        return (StepType::Lambda, None);
    }

    if meta.fail().is_some() {
        return (StepType::Fail, None);
    }

    (StepType::Unknown("Unknown".to_string()), None)
}

pub async fn list_steps(client: &Client, execution_arn: &str) -> Result<Vec<StepInfo>> {
    let resp = client
        .list_pipeline_execution_steps()
        .pipeline_execution_arn(execution_arn)
        .send()
        .await
        .context("Failed to list pipeline execution steps")?;

    let mut steps: Vec<StepInfo> = resp
        .pipeline_execution_steps()
        .iter()
        .map(|s| {
            let name = s.step_name().unwrap_or_default().to_string();
            let status = s
                .step_status()
                .map(|st| st.as_str().parse::<StepStatus>().unwrap())
                .unwrap_or(StepStatus::NotStarted);
            let start_time = s.start_time().and_then(to_chrono);
            let end_time = s.end_time().and_then(to_chrono);
            let failure_reason = s.failure_reason().map(|r| r.to_string());

            let (step_type, job_details) = s
                .metadata()
                .map(extract_step_type_and_job)
                .unwrap_or((StepType::Unknown("Unknown".to_string()), None));

            StepInfo {
                name,
                step_type,
                status,
                start_time,
                end_time,
                failure_reason,
                job_details,
            }
        })
        .collect();

    // Sort by start_time ascending; steps without a start time go to the end
    steps.sort_by(|a, b| match (&a.start_time, &b.start_time) {
        (Some(ta), Some(tb)) => ta.cmp(tb),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    Ok(steps)
}

/// Fetch final metrics from DescribeTrainingJob's final_metric_data_list.
pub async fn fetch_final_training_metrics(
    client: &Client,
    job_name: &str,
) -> Result<Vec<MetricDataPoint>> {
    let resp = client
        .describe_training_job()
        .training_job_name(job_name)
        .send()
        .await
        .context("Failed to describe training job for metrics")?;

    let points: Vec<MetricDataPoint> = resp
        .final_metric_data_list()
        .iter()
        .filter_map(|m| {
            let name = m.metric_name()?.to_string();
            let value = m.value()? as f64;
            let timestamp = m
                .timestamp()
                .and_then(|t| DateTime::from_timestamp(t.secs(), t.subsec_nanos()))?;
            Some(MetricDataPoint {
                metric_name: name,
                timestamp,
                value,
            })
        })
        .collect();

    Ok(points)
}

/// Fetch both final metrics and experiment time-series for a training job.
/// Discovers all metric names from the trial component, then fetches time-series for each.
pub async fn fetch_all_training_metrics(
    sagemaker_client: &Client,
    metrics_client: &MetricsClient,
    job_name: &str,
    job_arn: &str,
) -> Result<StepMetrics> {
    let final_metrics = fetch_final_training_metrics(sagemaker_client, job_name).await?;

    tracing::info!(
        job_name = %job_name,
        job_arn = %job_arn,
        final_metrics_count = final_metrics.len(),
        final_metric_names = ?final_metrics.iter().map(|m| &m.metric_name).collect::<Vec<_>>(),
        "fetching experiment metrics"
    );

    // Look up trial component ARN from training job ARN
    let experiment_series = match sagemaker_metrics::find_trial_component_arn(
        sagemaker_client,
        job_arn,
    )
    .await
    {
        Ok(Some(tc_arn)) => {
            tracing::info!(tc_arn = %tc_arn, "found trial component");

            // Discover all metric names from the trial component
            let metric_names =
                match sagemaker_metrics::discover_metric_names(sagemaker_client, &tc_arn).await {
                    Ok(names) => {
                        tracing::info!(
                            discovered_count = names.len(),
                            discovered_names = ?names,
                            "discovered metric names from trial component"
                        );
                        names
                    }
                    Err(e) => {
                        let fallback: Vec<String> = final_metrics
                            .iter()
                            .map(|m| m.metric_name.clone())
                            .collect();
                        tracing::warn!(
                            error = ?e,
                            fallback_count = fallback.len(),
                            "metric name discovery failed, falling back to final metrics"
                        );
                        fallback
                    }
                };

            if metric_names.is_empty() {
                tracing::warn!("no metric names to query");
                Vec::new()
            } else {
                tracing::info!(
                    querying_count = metric_names.len(),
                    "calling batch_get_metrics"
                );
                match sagemaker_metrics::fetch_experiment_metrics(
                    metrics_client,
                    &tc_arn,
                    &metric_names,
                )
                .await
                {
                    Ok(series) => {
                        tracing::info!(
                            series_with_data = series.len(),
                            series_names = ?series.iter().map(|s| &s.metric_name).collect::<Vec<_>>(),
                            "batch_get_metrics returned"
                        );
                        series
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "experiment metrics fetch failed");
                        Vec::new()
                    }
                }
            }
        }
        Ok(None) => {
            tracing::warn!(job_arn = %job_arn, "no trial component found for training job");
            Vec::new()
        }
        Err(e) => {
            tracing::warn!(error = %e, "trial component lookup failed");
            Vec::new()
        }
    };

    Ok(StepMetrics {
        final_metrics,
        experiment_series,
    })
}

pub async fn enrich_job_details(client: &Client, step: &mut StepInfo) -> Result<()> {
    let details = match &step.job_details {
        Some(d) => d,
        None => return Ok(()),
    };

    match details.job_type {
        JobType::Training => {
            let resp = client
                .describe_training_job()
                .training_job_name(&details.job_name)
                .send()
                .await
                .context("Failed to describe training job")?;

            if let Some(ref mut d) = step.job_details {
                d.secondary_status = resp.secondary_status().map(|s| s.as_str().to_string());
                d.instance_type = resp
                    .resource_config()
                    .and_then(|r| r.instance_type().map(|t| t.as_str().to_string()));
                d.instance_count = resp.resource_config().and_then(|r| r.instance_count());
            }
        }
        JobType::Processing => {
            let resp = client
                .describe_processing_job()
                .processing_job_name(&details.job_name)
                .send()
                .await
                .context("Failed to describe processing job")?;

            if let Some(ref mut d) = step.job_details {
                d.secondary_status = resp.processing_job_status().map(|s| s.as_str().to_string());
                d.instance_type = resp
                    .processing_resources()
                    .and_then(|r| r.cluster_config())
                    .and_then(|c| c.instance_type().map(|t| t.as_str().to_string()));
                d.instance_count = resp
                    .processing_resources()
                    .and_then(|r| r.cluster_config())
                    .and_then(|c| c.instance_count());
            }
        }
        JobType::Transform => {
            let resp = client
                .describe_transform_job()
                .transform_job_name(&details.job_name)
                .send()
                .await
                .context("Failed to describe transform job")?;

            if let Some(ref mut d) = step.job_details {
                d.secondary_status = resp.transform_job_status().map(|s| s.as_str().to_string());
                d.instance_type = resp
                    .transform_resources()
                    .and_then(|r| r.instance_type().map(|t| t.as_str().to_string()));
                d.instance_count = resp.transform_resources().and_then(|r| r.instance_count());
            }
        }
    }

    Ok(())
}
