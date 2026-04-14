use crate::aws::client::AwsClients;
use crate::model::execution::{ExecutionStatus, PipelineExecution};
use crate::model::step::{StepInfo, StepStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationEvent {
    StepSucceeded { step_name: String },
    StepFailed { step_name: String },
    PipelineSucceeded { pipeline_name: String },
    PipelineFailed { pipeline_name: String },
}

fn is_step_terminal(status: &StepStatus) -> bool {
    matches!(
        status,
        StepStatus::Succeeded | StepStatus::Failed | StepStatus::Stopped
    )
}

fn is_step_active(status: &StepStatus) -> bool {
    matches!(status, StepStatus::Executing | StepStatus::NotStarted)
}

fn is_execution_active(status: &ExecutionStatus) -> bool {
    matches!(
        status,
        ExecutionStatus::Executing | ExecutionStatus::Stopping
    )
}

fn is_execution_terminal(status: &ExecutionStatus) -> bool {
    matches!(
        status,
        ExecutionStatus::Succeeded | ExecutionStatus::Failed | ExecutionStatus::Stopped
    )
}

pub fn detect_step_transitions(old: &[StepInfo], new: &[StepInfo]) -> Vec<NotificationEvent> {
    let mut events = Vec::new();
    for new_step in new {
        if !is_step_terminal(&new_step.status) {
            continue;
        }
        // Find matching old step by name
        let Some(old_step) = old.iter().find(|s| s.name == new_step.name) else {
            continue;
        };
        if !is_step_active(&old_step.status) {
            continue;
        }
        match new_step.status {
            StepStatus::Succeeded => events.push(NotificationEvent::StepSucceeded {
                step_name: new_step.name.clone(),
            }),
            StepStatus::Failed | StepStatus::Stopped => {
                events.push(NotificationEvent::StepFailed {
                    step_name: new_step.name.clone(),
                })
            }
            _ => {}
        }
    }
    events
}

pub fn detect_execution_transition(
    old: &PipelineExecution,
    new: &PipelineExecution,
    pipeline_name: &str,
) -> Option<NotificationEvent> {
    if !is_execution_active(&old.status) || !is_execution_terminal(&new.status) {
        return None;
    }
    match new.status {
        ExecutionStatus::Succeeded => Some(NotificationEvent::PipelineSucceeded {
            pipeline_name: pipeline_name.to_string(),
        }),
        ExecutionStatus::Failed | ExecutionStatus::Stopped => {
            Some(NotificationEvent::PipelineFailed {
                pipeline_name: pipeline_name.to_string(),
            })
        }
        _ => None,
    }
}

pub fn spawn_background_watcher(
    clients: AwsClients,
    execution_arn: String,
    pipeline_name: String,
    initial_steps: Vec<StepInfo>,
    initial_execution: Option<PipelineExecution>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut prev_steps = initial_steps;
        let mut prev_execution = initial_execution;

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;

            // Fetch current execution status
            let execution =
                match crate::aws::sagemaker::describe_execution(&clients.sagemaker, &execution_arn)
                    .await
                {
                    Ok(exec) => exec,
                    Err(_) => continue,
                };

            // Fetch current steps
            let steps =
                match crate::aws::sagemaker::list_steps(&clients.sagemaker, &execution_arn).await {
                    Ok(s) => s,
                    Err(_) => continue,
                };

            // Detect and send step transition notifications
            let step_events = detect_step_transitions(&prev_steps, &steps);
            for event in &step_events {
                send(event);
            }

            // Detect and send execution transition notifications
            if let Some(ref old_exec) = prev_execution {
                if let Some(event) =
                    detect_execution_transition(old_exec, &execution, &pipeline_name)
                {
                    send(&event);
                }
            }

            // Check if execution reached terminal state
            let terminal = is_execution_terminal(&execution.status);

            prev_steps = steps;
            prev_execution = Some(execution);

            if terminal {
                break;
            }
        }
    })
}

pub fn send(event: &NotificationEvent) {
    let (summary, body) = match event {
        NotificationEvent::StepSucceeded { step_name } => {
            ("Step Succeeded".to_string(), step_name.clone())
        }
        NotificationEvent::StepFailed { step_name } => {
            ("Step Failed".to_string(), step_name.clone())
        }
        NotificationEvent::PipelineSucceeded { pipeline_name } => {
            ("Pipeline Succeeded".to_string(), pipeline_name.clone())
        }
        NotificationEvent::PipelineFailed { pipeline_name } => {
            ("Pipeline Failed".to_string(), pipeline_name.clone())
        }
    };

    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new()
            .appname("Sagecue")
            .summary(&summary)
            .body(&body)
            .show();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::step::{StepStatus, StepType};

    fn make_step(name: &str, status: StepStatus) -> StepInfo {
        StepInfo {
            name: name.to_string(),
            step_type: StepType::Training,
            status,
            start_time: None,
            end_time: None,
            failure_reason: None,
            job_details: None,
        }
    }

    fn make_execution(status: ExecutionStatus) -> PipelineExecution {
        PipelineExecution {
            display_name: None,
            status,
            created: None,
            last_modified: None,
        }
    }

    // --- Step transitions ---

    #[test]
    fn step_executing_to_succeeded() {
        let old = vec![make_step("Train", StepStatus::Executing)];
        let new = vec![make_step("Train", StepStatus::Succeeded)];
        let events = detect_step_transitions(&old, &new);
        assert_eq!(
            events,
            vec![NotificationEvent::StepSucceeded {
                step_name: "Train".into()
            }]
        );
    }

    #[test]
    fn step_executing_to_failed() {
        let old = vec![make_step("Train", StepStatus::Executing)];
        let new = vec![make_step("Train", StepStatus::Failed)];
        let events = detect_step_transitions(&old, &new);
        assert_eq!(
            events,
            vec![NotificationEvent::StepFailed {
                step_name: "Train".into()
            }]
        );
    }

    #[test]
    fn step_not_started_to_stopped() {
        let old = vec![make_step("Eval", StepStatus::NotStarted)];
        let new = vec![make_step("Eval", StepStatus::Stopped)];
        let events = detect_step_transitions(&old, &new);
        assert_eq!(
            events,
            vec![NotificationEvent::StepFailed {
                step_name: "Eval".into()
            }]
        );
    }

    #[test]
    fn step_already_terminal_no_event() {
        let old = vec![make_step("Train", StepStatus::Succeeded)];
        let new = vec![make_step("Train", StepStatus::Succeeded)];
        let events = detect_step_transitions(&old, &new);
        assert!(events.is_empty());
    }

    #[test]
    fn step_new_step_no_event() {
        let old = vec![];
        let new = vec![make_step("Train", StepStatus::Succeeded)];
        let events = detect_step_transitions(&old, &new);
        assert!(events.is_empty());
    }

    #[test]
    fn multiple_step_transitions() {
        let old = vec![
            make_step("A", StepStatus::Executing),
            make_step("B", StepStatus::Executing),
            make_step("C", StepStatus::Succeeded),
        ];
        let new = vec![
            make_step("A", StepStatus::Succeeded),
            make_step("B", StepStatus::Failed),
            make_step("C", StepStatus::Succeeded),
        ];
        let events = detect_step_transitions(&old, &new);
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0],
            NotificationEvent::StepSucceeded {
                step_name: "A".into()
            }
        );
        assert_eq!(
            events[1],
            NotificationEvent::StepFailed {
                step_name: "B".into()
            }
        );
    }

    // --- Execution transitions ---

    #[test]
    fn execution_executing_to_succeeded() {
        let old = make_execution(ExecutionStatus::Executing);
        let new = make_execution(ExecutionStatus::Succeeded);
        let event = detect_execution_transition(&old, &new, "my-pipeline");
        assert_eq!(
            event,
            Some(NotificationEvent::PipelineSucceeded {
                pipeline_name: "my-pipeline".into()
            })
        );
    }

    #[test]
    fn execution_stopping_to_failed() {
        let old = make_execution(ExecutionStatus::Stopping);
        let new = make_execution(ExecutionStatus::Failed);
        let event = detect_execution_transition(&old, &new, "pipe");
        assert_eq!(
            event,
            Some(NotificationEvent::PipelineFailed {
                pipeline_name: "pipe".into()
            })
        );
    }

    #[test]
    fn execution_already_terminal_no_event() {
        let old = make_execution(ExecutionStatus::Succeeded);
        let new = make_execution(ExecutionStatus::Succeeded);
        let event = detect_execution_transition(&old, &new, "pipe");
        assert_eq!(event, None);
    }

    #[test]
    fn execution_still_executing_no_event() {
        let old = make_execution(ExecutionStatus::Executing);
        let new = make_execution(ExecutionStatus::Executing);
        let event = detect_execution_transition(&old, &new, "pipe");
        assert_eq!(event, None);
    }
}
