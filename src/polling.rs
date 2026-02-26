use std::time::Duration;
use tokio::sync::{mpsc, watch};

use crate::aws::client::AwsClients;
use crate::aws::{cloudwatch, sagemaker};
use crate::model::execution::PipelineExecution;
use crate::model::logs::LogStreamState;
use crate::model::step::{StepInfo, StepStatus};

/// Result sent from poll task back to the main loop
#[derive(Debug)]
pub struct PollResult {
    pub execution: PipelineExecution,
    pub steps: Vec<StepInfo>,
    pub log_step_name: Option<String>,
    pub log_stream_state: Option<LogStreamState>,
}

/// Spawn the background polling task.
///
/// - `execution_arn_rx`: receives the current execution ARN to poll
/// - `selected_step_rx`: receives which step to tail logs for
/// - `result_tx`: sends poll results back to the main loop
pub fn spawn_poll_task(
    clients: AwsClients,
    execution_arn_rx: watch::Receiver<String>,
    selected_step_rx: watch::Receiver<String>,
    result_tx: mpsc::UnboundedSender<PollResult>,
    mut force_rx: mpsc::UnboundedReceiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        // Per-step log stream state, keyed by step name
        let mut log_states: std::collections::HashMap<String, LogStreamState> =
            std::collections::HashMap::new();

        loop {
            // Wait for next tick or force refresh
            tokio::select! {
                _ = interval.tick() => {}
                _ = force_rx.recv() => {
                    // Reset interval so next auto-tick is from now
                    interval.reset();
                }
            }

            let arn = execution_arn_rx.borrow().clone();
            if arn.is_empty() {
                continue;
            }

            let selected_step = selected_step_rx.borrow().clone();

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

            let result = PollResult {
                execution,
                steps,
                log_step_name,
                log_stream_state: log_stream_state_out,
            };

            if result_tx.send(result).is_err() {
                break;
            }
        }
    })
}
