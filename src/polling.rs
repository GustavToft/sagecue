use std::time::Duration;
use tokio::sync::{mpsc, watch};

use crate::aws::client::AwsClients;
use crate::aws::{cloudwatch, sagemaker};
use crate::model::execution::{ExecutionSummary, PipelineExecution};
use crate::model::logs::LogStreamState;
use crate::model::metrics::StepMetrics;
use crate::model::step::{JobDetails, JobType, StepInfo, StepStatus};

/// Configuration sent from the main loop to the poll task via a single watch channel.
///
/// The poller dispatches based on which field is populated:
/// - `execution_arn` non-empty → poll monitoring details for that execution
/// - else `list_pipeline_name` non-empty → poll the execution list for that pipeline
/// - else idle
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PollConfig {
    pub execution_arn: String,
    pub selected_step: String,
    pub metrics_tab_active: bool,
    pub list_pipeline_name: String,
}

/// Update from a successful monitoring poll.
#[derive(Debug)]
pub struct MonitoringUpdate {
    pub execution: PipelineExecution,
    pub steps: Vec<StepInfo>,
    pub log_step_name: Option<String>,
    pub log_stream_state: Option<LogStreamState>,
    pub metrics_step_name: Option<String>,
    pub metrics: Option<StepMetrics>,
}

/// Classification of a poll error. Credential/expiration errors are surfaced
/// with a distinct variant so the UI can show a clearer message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollError {
    CredentialsExpired { message: String },
    Other { message: String },
}

impl PollError {
    pub fn message(&self) -> &str {
        match self {
            PollError::CredentialsExpired { message } => message,
            PollError::Other { message } => message,
        }
    }
}

/// Result sent from the poll task back to the main loop.
///
/// `Monitoring` is boxed because it's much larger than the other variants
/// and the enum is passed through an mpsc channel.
#[derive(Debug)]
pub enum PollResult {
    Monitoring(Box<MonitoringUpdate>),
    ExecutionList {
        pipeline_name: String,
        executions: Vec<ExecutionSummary>,
    },
    Error(PollError),
}

/// AWS SDK error-code / message fragments that indicate expired or invalid credentials.
const CREDENTIAL_MARKERS: &[&str] = &[
    "ExpiredToken",
    "ExpiredTokenException",
    "UnrecognizedClientException",
    "InvalidClientTokenId",
    "CredentialsProviderError",
    "credentials",
    "token has expired",
];

/// Walk an error chain and classify it into a `PollError`.
pub fn classify(err: &anyhow::Error) -> PollError {
    let mut full = String::new();
    for (i, cause) in err.chain().enumerate() {
        if i > 0 {
            full.push_str(": ");
        }
        full.push_str(&cause.to_string());
    }
    let lower = full.to_lowercase();
    for marker in CREDENTIAL_MARKERS {
        if lower.contains(&marker.to_lowercase()) {
            return PollError::CredentialsExpired { message: full };
        }
    }
    PollError::Other { message: full }
}

/// Spawn the background polling task.
pub fn spawn_poll_task(
    clients: AwsClients,
    config_rx: watch::Receiver<PollConfig>,
    result_tx: mpsc::UnboundedSender<PollResult>,
    mut force_rx: mpsc::UnboundedReceiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        // Per-step log stream state, keyed by step name
        let mut log_states: std::collections::HashMap<String, LogStreamState> =
            std::collections::HashMap::new();
        // Cache of enriched job details, keyed by job name. Lets us backfill
        // instance type / count for completed steps (which we otherwise skip)
        // without re-calling describe_* on every tick.
        let mut job_detail_cache: std::collections::HashMap<String, JobDetails> =
            std::collections::HashMap::new();
        let mut last_arn = String::new();

        loop {
            // Wait for next tick or force refresh
            tokio::select! {
                _ = interval.tick() => {}
                _ = force_rx.recv() => {
                    // Reset interval so next auto-tick is from now
                    interval.reset();
                }
            }

            let config = config_rx.borrow().clone();
            let arn = config.execution_arn;

            if arn.is_empty() {
                // No monitoring target — fall back to polling the execution list
                // if the SelectExecution screen has set a pipeline name.
                if !config.list_pipeline_name.is_empty() {
                    let pipeline_name = config.list_pipeline_name.clone();
                    match sagemaker::list_executions(&clients.sagemaker, &pipeline_name, 20).await {
                        Ok(executions) => {
                            if result_tx
                                .send(PollResult::ExecutionList {
                                    pipeline_name,
                                    executions,
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = result_tx.send(PollResult::Error(classify(&e)));
                        }
                    }
                }
                continue;
            }

            // Clear cached state when switching to a different execution
            if arn != last_arn {
                log_states.clear();
                job_detail_cache.clear();
                last_arn = arn.clone();
            }

            let selected_step = config.selected_step;
            let metrics_tab_active = config.metrics_tab_active;

            tracing::debug!(
                step = %selected_step,
                metrics_tab = metrics_tab_active,
                "poll tick"
            );

            // Poll SageMaker
            let execution = match sagemaker::describe_execution(&clients.sagemaker, &arn).await {
                Ok(e) => e,
                Err(e) => {
                    let _ = result_tx.send(PollResult::Error(classify(&e)));
                    continue;
                }
            };

            let mut steps = match sagemaker::list_steps(&clients.sagemaker, &arn).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = result_tx.send(PollResult::Error(classify(&e)));
                    continue;
                }
            };

            // Enrich job details:
            //   - Executing steps: always refresh (for live secondary_status).
            //   - Terminal steps: enrich once, then reuse from cache so we
            //     don't hammer describe_* APIs on every tick.
            // In both cases, update the cache after a successful enrichment
            // and paste cached data into any step we skipped.
            for step in &mut steps {
                let Some(ref details) = step.job_details else {
                    continue;
                };
                let job_name = details.job_name.clone();
                let is_executing = step.status == StepStatus::Executing;
                let cached = job_detail_cache.get(&job_name).cloned();
                let needs_enrich = is_executing || cached.is_none();

                if needs_enrich {
                    if sagemaker::enrich_job_details(&clients.sagemaker, step)
                        .await
                        .is_ok()
                    {
                        if let Some(ref d) = step.job_details {
                            job_detail_cache.insert(job_name, d.clone());
                        }
                    } else if let Some(cached_details) = cached {
                        // Enrichment failed; fall back to cached data if we
                        // have any rather than showing blanks.
                        step.job_details = Some(cached_details);
                    }
                } else if let Some(cached_details) = cached {
                    step.job_details = Some(cached_details);
                }
            }

            // Tail logs for the selected step
            let mut log_step_name = None;
            let mut log_stream_state_out = None;

            if !selected_step.is_empty() {
                if let Some(step) = steps.iter().find(|s| s.name == selected_step) {
                    if let Some(ref job) = step.job_details {
                        let state = log_states
                            .entry(selected_step.clone())
                            .or_insert_with(|| LogStreamState::new(String::new()));

                        // Discover stream if not yet found
                        if state.log_stream.is_none() {
                            if let Ok(Some(discovered)) =
                                cloudwatch::discover_log_stream(&clients.cloudwatch_logs, job).await
                            {
                                *state = discovered;
                            }
                        }

                        // Fetch new events
                        if state.log_stream.is_some() {
                            if let Ok(entries) =
                                cloudwatch::fetch_log_events(&clients.cloudwatch_logs, state).await
                            {
                                state.entries.extend(entries);
                            }
                        }

                        log_step_name = Some(selected_step.clone());
                        log_stream_state_out = Some(state.clone());
                    }
                }
            }

            // Fetch training metrics when Metrics tab is active
            let mut metrics_step_name = None;
            let mut metrics_out = None;

            if metrics_tab_active && !selected_step.is_empty() {
                if let Some(step) = steps.iter().find(|s| s.name == selected_step) {
                    if let Some(ref job) = step.job_details {
                        if matches!(job.job_type, JobType::Training) {
                            let job_arn = job.job_arn.as_deref().unwrap_or_default();
                            tracing::debug!(job_name = %job.job_name, "fetching training metrics");

                            match sagemaker::fetch_all_training_metrics(
                                &clients.sagemaker,
                                &clients.sagemaker_metrics,
                                &job.job_name,
                                job_arn,
                            )
                            .await
                            {
                                Ok(step_metrics) => {
                                    tracing::debug!(
                                        final_count = step_metrics.final_metrics.len(),
                                        experiment_series_count =
                                            step_metrics.experiment_series.len(),
                                        "metrics fetched"
                                    );

                                    metrics_step_name = Some(selected_step.clone());
                                    metrics_out = Some(step_metrics);
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "training metrics fetch failed");
                                }
                            }
                        }
                    }
                }
            }

            let update = MonitoringUpdate {
                execution,
                steps,
                log_step_name,
                log_stream_state: log_stream_state_out,
                metrics_step_name,
                metrics: metrics_out,
            };

            if result_tx
                .send(PollResult::Monitoring(Box::new(update)))
                .is_err()
            {
                break;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{anyhow, Context};

    #[test]
    fn classify_expired_token() {
        let err = anyhow!("ExpiredToken: The security token included in the request is expired");
        assert!(matches!(
            classify(&err),
            PollError::CredentialsExpired { .. }
        ));
    }

    #[test]
    fn classify_expired_token_exception() {
        let err = anyhow!("ExpiredTokenException: token expired");
        assert!(matches!(
            classify(&err),
            PollError::CredentialsExpired { .. }
        ));
    }

    #[test]
    fn classify_unrecognized_client() {
        let err = anyhow!(
            "UnrecognizedClientException: The security token included in the request is invalid"
        );
        assert!(matches!(
            classify(&err),
            PollError::CredentialsExpired { .. }
        ));
    }

    #[test]
    fn classify_invalid_client_token_id() {
        let err = anyhow!("InvalidClientTokenId: bad token");
        assert!(matches!(
            classify(&err),
            PollError::CredentialsExpired { .. }
        ));
    }

    #[test]
    fn classify_credentials_provider_error() {
        let err = anyhow!("CredentialsProviderError: no provider");
        assert!(matches!(
            classify(&err),
            PollError::CredentialsExpired { .. }
        ));
    }

    #[test]
    fn classify_expired_marker_deep_in_chain() {
        // Wrap a root cause so it only appears via Error::chain, not Display.
        let root: anyhow::Error = anyhow!("ExpiredTokenException: expired");
        let wrapped = Err::<(), _>(root)
            .context("Failed to describe pipeline execution")
            .unwrap_err();
        assert!(matches!(
            classify(&wrapped),
            PollError::CredentialsExpired { .. }
        ));
    }

    #[test]
    fn classify_generic_error_is_other() {
        let err = anyhow!("boom: something went wrong");
        match classify(&err) {
            PollError::Other { message } => {
                assert!(message.contains("boom"));
            }
            other => panic!("expected Other, got {:?}", other),
        }
    }

    #[test]
    fn classify_preserves_full_message() {
        let err = anyhow!("network unreachable");
        let e = classify(&err);
        assert_eq!(e.message(), "network unreachable");
    }
}
