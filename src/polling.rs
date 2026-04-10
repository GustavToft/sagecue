use std::time::Duration;
use tokio::sync::{mpsc, watch};

use crate::aws::client::AwsClients;
use crate::aws::{cloudwatch, sagemaker};
use crate::model::execution::PipelineExecution;
use crate::model::logs::LogStreamState;
use crate::model::metrics::StepMetrics;
use crate::model::step::{JobType, StepInfo, StepStatus};

/// Configuration sent from the main loop to the poll task via a single watch channel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PollConfig {
    pub execution_arn: String,
    pub selected_step: String,
    pub metrics_tab_active: bool,
}

/// Result sent from poll task back to the main loop
#[derive(Debug)]
pub struct PollResult {
    pub execution: PipelineExecution,
    pub steps: Vec<StepInfo>,
    pub log_step_name: Option<String>,
    pub log_stream_state: Option<LogStreamState>,
    pub metrics_step_name: Option<String>,
    pub metrics: Option<StepMetrics>,
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
                continue;
            }

            // Clear cached state when switching to a different execution
            if arn != last_arn {
                log_states.clear();
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
                Err(_) => continue,
            };

            let mut steps = match sagemaker::list_steps(&clients.sagemaker, &arn).await {
                Ok(s) => s,
                Err(_) => continue,
            };

            // Enrich executing steps with job details
            for step in &mut steps {
                if step.status == StepStatus::Executing && step.job_details.is_some() {
                    let _ = sagemaker::enrich_job_details(&clients.sagemaker, step).await;
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
                            .or_insert_with(|| {
                                LogStreamState::new(String::new())
                            });

                        // Discover stream if not yet found
                        if state.log_stream.is_none() {
                            if let Ok(Some(discovered)) =
                                cloudwatch::discover_log_stream(
                                    &clients.cloudwatch_logs,
                                    job,
                                )
                                .await
                            {
                                *state = discovered;
                            }
                        }

                        // Fetch new events
                        if state.log_stream.is_some() {
                            if let Ok(entries) =
                                cloudwatch::fetch_log_events(
                                    &clients.cloudwatch_logs,
                                    state,
                                )
                                .await
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
                                        experiment_series_count = step_metrics.experiment_series.len(),
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

            let result = PollResult {
                execution,
                steps,
                log_step_name,
                log_stream_state: log_stream_state_out,
                metrics_step_name,
                metrics: metrics_out,
            };

            if result_tx.send(result).is_err() {
                break;
            }
        }
    })
}
