use anyhow::{Context, Result};
use aws_sdk_sagemaker::Client;
use chrono::{DateTime, Utc};

use crate::model::execution::{ExecutionStatus, ExecutionSummary, PipelineExecution};
use crate::model::pipeline::PipelineSummary;
use crate::model::step::{JobDetails, JobType, StepInfo, StepStatus, PIPELINE_STEPS};

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
            arn: p.pipeline_arn().unwrap_or_default().to_string(),
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
                .map(|s| ExecutionStatus::from_str(s.as_str()))
                .unwrap_or(ExecutionStatus::Unknown("Unknown".to_string())),
            start_time: s.start_time().and_then(to_chrono),
        })
        .collect();

    Ok(summaries)
}

pub async fn describe_execution(
    client: &Client,
    execution_arn: &str,
) -> Result<PipelineExecution> {
    let resp = client
        .describe_pipeline_execution()
        .pipeline_execution_arn(execution_arn)
        .send()
        .await
        .context("Failed to describe pipeline execution")?;

    Ok(PipelineExecution {
        arn: execution_arn.to_string(),
        display_name: resp
            .pipeline_execution_display_name()
            .map(|s| s.to_string()),
        status: resp
            .pipeline_execution_status()
            .map(|s| ExecutionStatus::from_str(s.as_str()))
            .unwrap_or(ExecutionStatus::Unknown("Unknown".to_string())),
        created: resp.creation_time().and_then(to_chrono),
        last_modified: resp.last_modified_time().and_then(to_chrono),
    })
}

pub async fn list_steps(client: &Client, execution_arn: &str) -> Result<Vec<StepInfo>> {
    let resp = client
        .list_pipeline_execution_steps()
        .pipeline_execution_arn(execution_arn)
        .send()
        .await
        .context("Failed to list pipeline execution steps")?;

    let api_steps = resp.pipeline_execution_steps();

    // Build step info for each known pipeline step, preserving order
    let steps = PIPELINE_STEPS
        .iter()
        .map(|&name| {
            let api_step = api_steps
                .iter()
                .find(|s| s.step_name().unwrap_or_default() == name);

            match api_step {
                Some(s) => {
                    let status = s
                        .step_status()
                        .map(|st| StepStatus::from_str(st.as_str()))
                        .unwrap_or(StepStatus::NotStarted);

                    let start_time = s.start_time().and_then(to_chrono);
                    let end_time = s.end_time().and_then(to_chrono);
                    let failure_reason = s.failure_reason().map(|r| r.to_string());

                    // Extract job ARN from metadata
                    let job_details = s.metadata().and_then(|meta| {
                        if let Some(training) = meta.training_job() {
                            let arn = training.arn()?.to_string();
                            let job_name = arn.rsplit('/').next()?.to_string();
                            Some(JobDetails {
                                job_type: JobType::Training,
                                job_arn: arn,
                                job_name,
                                secondary_status: None,
                                instance_type: None,
                            })
                        } else if let Some(processing) = meta.processing_job() {
                            let arn = processing.arn()?.to_string();
                            let job_name = arn.rsplit('/').next()?.to_string();
                            Some(JobDetails {
                                job_type: JobType::Processing,
                                job_arn: arn,
                                job_name,
                                secondary_status: None,
                                instance_type: None,
                            })
                        } else {
                            None
                        }
                    });

                    StepInfo {
                        name: name.to_string(),
                        status,
                        start_time,
                        end_time,
                        failure_reason,
                        job_details,
                    }
                }
                None => StepInfo::new(name),
            }
        })
        .collect();

    Ok(steps)
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
                d.secondary_status = resp
                    .secondary_status()
                    .map(|s| s.as_str().to_string());
                d.instance_type = resp
                    .resource_config()
                    .and_then(|r| r.instance_type().map(|t| t.as_str().to_string()));
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
                d.secondary_status = resp
                    .processing_job_status()
                    .map(|s| s.as_str().to_string());
                d.instance_type = resp
                    .processing_resources()
                    .and_then(|r| r.cluster_config())
                    .and_then(|c| c.instance_type().map(|t| t.as_str().to_string()));
            }
        }
    }

    Ok(())
}
