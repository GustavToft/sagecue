use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use super::execution::{ExecutionStatus, PipelineExecution};
use super::format::format_duration;
use super::step::{JobDetails, JobType, StepInfo, StepType};

/// Identifies a standalone SageMaker job. Job names are only unique per job
/// type, so the type travels with the name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRef {
    pub job_type: JobType,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct JobSummary {
    pub name: String,
    pub job_type: JobType,
    pub status: ExecutionStatus,
    pub created: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
}

impl JobSummary {
    pub fn job_ref(&self) -> JobRef {
        JobRef {
            job_type: self.job_type.clone(),
            name: self.name.clone(),
        }
    }

    pub fn duration_str(&self) -> String {
        let Some(start) = self.created else {
            return "--".to_string();
        };
        let end = self.end_time.unwrap_or_else(Utc::now);
        format_duration((end - start).num_seconds())
    }
}

/// Keep the `limit` most recently created jobs, newest first. SageMaker lists
/// each job type separately, so this merges them into one timeline.
pub fn most_recent(mut jobs: Vec<JobSummary>, limit: usize) -> Vec<JobSummary> {
    jobs.sort_by_key(|job| std::cmp::Reverse(job.created));
    jobs.truncate(limit);
    jobs
}

/// A standalone job's Describe*Job response, flattened across job types.
#[derive(Debug, Clone)]
pub struct JobDescription {
    pub details: JobDetails,
    pub status: ExecutionStatus,
    pub created: Option<DateTime<Utc>>,
    pub last_modified: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub failure_reason: Option<String>,
    pub parameters: BTreeMap<String, String>,
}

impl JobDescription {
    /// Present the job as a one-step execution so the pipeline monitor's
    /// header, step table, and Logs/Metrics/Instance tabs render it unchanged.
    /// The step is named after the job.
    pub fn into_monitor_view(self) -> (PipelineExecution, StepInfo) {
        let step_type = match self.details.job_type {
            JobType::Training => StepType::Training,
            JobType::Processing => StepType::Processing,
            JobType::Transform => StepType::Transform,
        };
        // Pipeline steps and executions share status names, so the mapped
        // execution status parses straight into a step status.
        let step = StepInfo {
            name: self.details.job_name.clone(),
            step_type,
            status: self.status.as_str().parse().unwrap(),
            // Creation, not training start, so time spent waiting for
            // capacity counts toward the duration like it does for steps.
            start_time: self.created,
            end_time: self.end_time,
            failure_reason: self.failure_reason.clone(),
            job_details: Some(self.details.clone()),
        };
        let execution = PipelineExecution {
            display_name: Some(self.details.job_name),
            status: self.status,
            created: self.created,
            last_modified: self.last_modified,
            failure_reason: self.failure_reason,
            parameters: self.parameters,
        };
        (execution, step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::step::StepStatus;
    use chrono::TimeZone;

    fn summary(name: &str, created_secs: Option<i64>) -> JobSummary {
        JobSummary {
            name: name.to_string(),
            job_type: JobType::Training,
            status: ExecutionStatus::Succeeded,
            created: created_secs.map(|s| Utc.timestamp_opt(s, 0).unwrap()),
            end_time: None,
        }
    }

    fn description(job_type: JobType, status: ExecutionStatus) -> JobDescription {
        JobDescription {
            details: JobDetails {
                job_type,
                job_name: "my-job".to_string(),
                job_arn: Some("arn:job".to_string()),
                secondary_status: None,
                status_message: None,
                managed_spot: None,
                instance_type: Some("ml.m5.large".to_string()),
                instance_count: Some(1),
            },
            status,
            created: Some(Utc.timestamp_opt(1_000, 0).unwrap()),
            last_modified: Some(Utc.timestamp_opt(2_000, 0).unwrap()),
            end_time: None,
            failure_reason: None,
            parameters: BTreeMap::new(),
        }
    }

    #[test]
    fn most_recent_sorts_newest_first_across_types() {
        let mut processing = summary("p", Some(300));
        processing.job_type = JobType::Processing;
        let jobs = vec![
            summary("old", Some(100)),
            processing,
            summary("mid", Some(200)),
        ];
        let names: Vec<_> = most_recent(jobs, 10).into_iter().map(|j| j.name).collect();
        assert_eq!(names, vec!["p", "mid", "old"]);
    }

    #[test]
    fn most_recent_truncates_to_limit() {
        let jobs = vec![
            summary("a", Some(1)),
            summary("b", Some(2)),
            summary("c", Some(3)),
        ];
        let kept = most_recent(jobs, 2);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].name, "c");
        assert_eq!(kept[1].name, "b");
    }

    #[test]
    fn most_recent_puts_missing_creation_time_last() {
        let jobs = vec![summary("none", None), summary("some", Some(1))];
        let kept = most_recent(jobs, 10);
        assert_eq!(kept[0].name, "some");
        assert_eq!(kept[1].name, "none");
    }

    #[test]
    fn duration_str_uses_end_time() {
        let mut job = summary("j", Some(0));
        job.end_time = Some(Utc.timestamp_opt(90, 0).unwrap());
        assert_eq!(job.duration_str(), "1m 30s");
    }

    #[test]
    fn duration_str_without_creation_time() {
        assert_eq!(summary("j", None).duration_str(), "--");
    }

    #[test]
    fn job_ref_carries_name_and_type() {
        let mut job = summary("j", None);
        job.job_type = JobType::Transform;
        assert_eq!(
            job.job_ref(),
            JobRef {
                job_type: JobType::Transform,
                name: "j".to_string(),
            }
        );
    }

    #[test]
    fn monitor_view_names_step_after_job() {
        let (execution, step) =
            description(JobType::Processing, ExecutionStatus::Executing).into_monitor_view();
        assert_eq!(step.name, "my-job");
        assert!(matches!(step.step_type, StepType::Processing));
        assert_eq!(step.status, StepStatus::Executing);
        assert_eq!(execution.display_name.as_deref(), Some("my-job"));
        assert_eq!(execution.status, ExecutionStatus::Executing);
        assert_eq!(step.instance_str(), "ml.m5.large");
    }

    #[test]
    fn monitor_view_starts_step_at_creation() {
        let desc = description(JobType::Training, ExecutionStatus::Executing);
        let created = desc.created;
        let (execution, step) = desc.into_monitor_view();
        assert_eq!(step.start_time, created);
        assert_eq!(execution.created, created);
    }

    #[test]
    fn monitor_view_propagates_failure() {
        let mut desc = description(JobType::Training, ExecutionStatus::Failed);
        desc.failure_reason = Some("OOM".to_string());
        let (execution, step) = desc.into_monitor_view();
        assert_eq!(step.status, StepStatus::Failed);
        assert_eq!(step.failure_reason.as_deref(), Some("OOM"));
        assert_eq!(execution.failure_reason.as_deref(), Some("OOM"));
    }

    #[test]
    fn monitor_view_stopping_job_has_stopping_step() {
        let (_, step) =
            description(JobType::Transform, ExecutionStatus::Stopping).into_monitor_view();
        assert_eq!(step.status, StepStatus::Unknown("Stopping".to_string()));
    }

    #[test]
    fn monitor_view_keeps_parameters() {
        let mut desc = description(JobType::Training, ExecutionStatus::Succeeded);
        desc.parameters
            .insert("epochs".to_string(), "10".to_string());
        let (execution, _) = desc.into_monitor_view();
        assert_eq!(
            execution.parameters.get("epochs").map(String::as_str),
            Some("10")
        );
    }
}
