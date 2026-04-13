use anyhow::{Context, Result};
use aws_sdk_cloudwatchlogs::Client;

use crate::model::logs::{LogEntry, LogStreamState};
use crate::model::step::{JobDetails, JobType};

fn log_group_for_job(job: &JobDetails) -> &'static str {
    match job.job_type {
        JobType::Training => "/aws/sagemaker/TrainingJobs",
        JobType::Processing => "/aws/sagemaker/ProcessingJobs",
        JobType::Transform => "/aws/sagemaker/TransformJobs",
    }
}

pub async fn discover_log_stream(
    client: &Client,
    job: &JobDetails,
) -> Result<Option<LogStreamState>> {
    let log_group = log_group_for_job(job);

    let resp = client
        .describe_log_streams()
        .log_group_name(log_group)
        .log_stream_name_prefix(&job.job_name)
        .order_by(aws_sdk_cloudwatchlogs::types::OrderBy::LogStreamName)
        .limit(5)
        .send()
        .await
        .context("Failed to describe log streams")?;

    let streams = resp.log_streams();
    if streams.is_empty() {
        return Ok(None);
    }

    // Pick the first matching stream (usually algo-1 for training)
    let stream_name = streams[0].log_stream_name().unwrap_or_default().to_string();

    let mut state = LogStreamState::new(log_group.to_string());
    state.log_stream = Some(stream_name);
    Ok(Some(state))
}

pub async fn fetch_log_events(
    client: &Client,
    state: &mut LogStreamState,
) -> Result<Vec<LogEntry>> {
    let stream_name = match &state.log_stream {
        Some(s) => s.clone(),
        None => return Ok(Vec::new()),
    };

    let mut req = client
        .get_log_events()
        .log_group_name(&state.log_group)
        .log_stream_name(&stream_name)
        .start_from_head(true);

    if let Some(ref token) = state.next_forward_token {
        req = req.next_token(token);
    }

    let resp = req.send().await.context("Failed to get log events")?;

    let new_token = resp.next_forward_token().map(|t| t.to_string());
    let events: Vec<LogEntry> = resp
        .events()
        .iter()
        .map(|e| LogEntry {
            timestamp: e.timestamp().unwrap_or(0),
            message: e.message().unwrap_or_default().to_string(),
        })
        .collect();

    // Only update token if we got new events (avoid infinite loop on same token)
    if !events.is_empty() {
        state.next_forward_token = new_token;
    } else if state.next_forward_token.is_none() {
        // First call with no events — still save token for subsequent calls
        state.next_forward_token = new_token;
    }

    Ok(events)
}
