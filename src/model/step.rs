use chrono::{DateTime, Utc};

use super::format::{fmt_local, format_duration};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepStatus {
    NotStarted,
    Executing,
    Succeeded,
    Failed,
    Stopped,
    Unknown(String),
}

impl std::str::FromStr for StepStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "Executing" => Self::Executing,
            "Succeeded" => Self::Succeeded,
            "Failed" => Self::Failed,
            "Stopped" => Self::Stopped,
            other => Self::Unknown(other.to_string()),
        })
    }
}

impl StepStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::NotStarted => "Not Started",
            Self::Executing => "Executing",
            Self::Succeeded => "Succeeded",
            Self::Failed => "Failed",
            Self::Stopped => "Stopped",
            Self::Unknown(s) => s.as_str(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum JobType {
    Training,
    Processing,
    Transform,
}

#[derive(Debug, Clone)]
pub enum StepType {
    Training,
    Processing,
    Transform,
    Condition,
    RegisterModel,
    Lambda,
    Fail,
    Unknown(String),
}

impl StepType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Training => "Training",
            Self::Processing => "Processing",
            Self::Transform => "Transform",
            Self::Condition => "Condition",
            Self::RegisterModel => "RegisterModel",
            Self::Lambda => "Lambda",
            Self::Fail => "Fail",
            Self::Unknown(s) => s.as_str(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct JobDetails {
    pub job_type: JobType,
    pub job_name: String,
    #[allow(dead_code)]
    pub job_arn: Option<String>,
    pub secondary_status: Option<String>,
    /// Human-readable message from the latest `SecondaryStatusTransitions`
    /// entry (training jobs only), e.g. "Training job waiting for capacity".
    /// More explanatory than the bare `secondary_status` token.
    pub status_message: Option<String>,
    /// `EnableManagedSpotTraining` — whether the job is waiting on spot or
    /// on-demand capacity. The remediation for a capacity wait differs.
    pub managed_spot: Option<bool>,
    pub instance_type: Option<String>,
    pub instance_count: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct StepInfo {
    pub name: String,
    pub step_type: StepType,
    pub status: StepStatus,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub failure_reason: Option<String>,
    pub job_details: Option<JobDetails>,
}

impl StepInfo {
    pub fn duration_str(&self) -> String {
        let start = match self.start_time {
            Some(t) => t,
            None => return "--".to_string(),
        };
        let end = self.end_time.unwrap_or_else(Utc::now);
        format_duration((end - start).num_seconds())
    }

    pub fn start_time_str(&self) -> String {
        match self.start_time {
            Some(t) => fmt_local(t, "%H:%M:%S"),
            None => "--".to_string(),
        }
    }

    /// Render the instance type + count for display, e.g. `"4x ml.p3.8xl"` or
    /// just `"ml.m5.large"` for a single instance. Returns `"--"` for steps
    /// without job details or without a known instance type.
    pub fn instance_str(&self) -> String {
        let Some(ref d) = self.job_details else {
            return "--".to_string();
        };
        let Some(ref type_str) = d.instance_type else {
            return "--".to_string();
        };
        match d.instance_count {
            Some(n) if n > 1 => format!("{}x {}", n, type_str),
            _ => type_str.clone(),
        }
    }

    pub fn detail_str(&self) -> String {
        if let Some(ref details) = self.job_details {
            // Prefer the explanatory transition message ("Training job waiting
            // for capacity") over the bare status token ("Pending").
            if let Some(ref msg) = details.status_message {
                return msg.clone();
            }
            if let Some(ref status) = details.secondary_status {
                return status.clone();
            }
        }
        if let Some(ref reason) = self.failure_reason {
            let truncated: String = reason.chars().take(40).collect();
            if reason.len() > 40 {
                return format!("{}...", truncated);
            }
            return truncated;
        }
        String::new()
    }

    /// Text for the logs panel when there are no log entries yet.
    ///
    /// A training job stuck in a pre-instance state (e.g. `Pending` while AWS
    /// finds capacity) never creates a CloudWatch stream, so an empty panel
    /// looks broken. When we have a secondary-status message, surface it along
    /// with the instance type, capacity kind, and how long we've been waiting
    /// so the wait is explained rather than silent.
    pub fn empty_logs_message(&self) -> String {
        let Some(ref d) = self.job_details else {
            return "No logs available (step not started or no job)".to_string();
        };

        let Some(ref msg) = d.status_message else {
            return "Waiting for log stream...".to_string();
        };

        let mut parts: Vec<String> = Vec::new();
        let instance = self.instance_str();
        if instance != "--" {
            parts.push(instance);
        }
        if let Some(spot) = d.managed_spot {
            parts.push(if spot { "spot" } else { "on-demand" }.to_string());
        }
        if let Some(ref status) = d.secondary_status {
            parts.push(format!("{} {}", status.to_lowercase(), self.duration_str()));
        }

        let suffix = if parts.is_empty() {
            String::new()
        } else {
            format!(" ({})", parts.join(", "))
        };
        format!("No log stream yet — instance not provisioned.\nStatus: \"{msg}\"{suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_step(name: &str) -> StepInfo {
        StepInfo {
            name: name.to_string(),
            step_type: StepType::Training,
            status: StepStatus::Succeeded,
            start_time: None,
            end_time: None,
            failure_reason: None,
            job_details: None,
        }
    }

    // --- StepStatus ---

    fn parse_step_status(s: &str) -> StepStatus {
        s.parse().unwrap()
    }

    #[test]
    fn status_from_str_known_variants() {
        assert_eq!(parse_step_status("Executing"), StepStatus::Executing);
        assert_eq!(parse_step_status("Succeeded"), StepStatus::Succeeded);
        assert_eq!(parse_step_status("Failed"), StepStatus::Failed);
        assert_eq!(parse_step_status("Stopped"), StepStatus::Stopped);
    }

    #[test]
    fn status_from_str_unknown() {
        assert_eq!(
            parse_step_status("Banana"),
            StepStatus::Unknown("Banana".to_string())
        );
    }

    #[test]
    fn status_as_str_roundtrip() {
        for s in ["Executing", "Succeeded", "Failed", "Stopped"] {
            assert_eq!(parse_step_status(s).as_str(), s);
        }
    }

    #[test]
    fn status_not_started_as_str() {
        assert_eq!(StepStatus::NotStarted.as_str(), "Not Started");
    }

    // --- StepInfo::detail_str ---

    #[test]
    fn detail_str_empty_when_no_details() {
        let step = make_step("s");
        assert_eq!(step.detail_str(), "");
    }

    #[test]
    fn detail_str_secondary_status_takes_priority() {
        let mut step = make_step("s");
        step.failure_reason = Some("bad stuff".to_string());
        step.job_details = Some(JobDetails {
            job_type: JobType::Training,
            job_name: "job".to_string(),
            job_arn: None,
            secondary_status: Some("Downloading".to_string()),
            status_message: None,
            managed_spot: None,
            instance_type: None,
            instance_count: None,
        });
        assert_eq!(step.detail_str(), "Downloading");
    }

    #[test]
    fn detail_str_failure_reason_truncated_at_40() {
        let mut step = make_step("s");
        step.failure_reason = Some("a".repeat(50));
        let detail = step.detail_str();
        assert!(detail.ends_with("..."));
        // 40 chars + "..." = 43
        assert_eq!(detail.len(), 43);
    }

    #[test]
    fn detail_str_short_failure_reason_not_truncated() {
        let mut step = make_step("s");
        step.failure_reason = Some("short reason".to_string());
        assert_eq!(step.detail_str(), "short reason");
    }

    #[test]
    fn detail_str_prefers_status_message_over_secondary_status() {
        let mut step = make_step("s");
        step.job_details = Some(JobDetails {
            job_type: JobType::Training,
            job_name: "job".to_string(),
            job_arn: None,
            secondary_status: Some("Pending".to_string()),
            status_message: Some("Training job waiting for capacity".to_string()),
            managed_spot: None,
            instance_type: None,
            instance_count: None,
        });
        assert_eq!(step.detail_str(), "Training job waiting for capacity");
    }

    // --- StepInfo::empty_logs_message ---

    #[test]
    fn empty_logs_message_no_job_details() {
        let step = make_step("s");
        assert_eq!(
            step.empty_logs_message(),
            "No logs available (step not started or no job)"
        );
    }

    #[test]
    fn empty_logs_message_job_without_status_message() {
        let mut step = make_step("s");
        step.job_details = Some(make_job_details(Some("ml.m5.large"), Some(1)));
        assert_eq!(step.empty_logs_message(), "Waiting for log stream...");
    }

    #[test]
    fn empty_logs_message_waiting_for_capacity() {
        use chrono::Duration;
        let mut step = make_step("s");
        step.status = StepStatus::Executing;
        step.start_time = Some(Utc::now() - Duration::seconds(42 * 60));
        step.job_details = Some(JobDetails {
            job_type: JobType::Training,
            job_name: "job".to_string(),
            job_arn: None,
            secondary_status: Some("Pending".to_string()),
            status_message: Some("Training job waiting for capacity".to_string()),
            managed_spot: Some(false),
            instance_type: Some("ml.g5.xlarge".to_string()),
            instance_count: Some(1),
        });
        let msg = step.empty_logs_message();
        assert!(msg.starts_with("No log stream yet — instance not provisioned."));
        assert!(msg.contains("Status: \"Training job waiting for capacity\""));
        assert!(msg.contains("ml.g5.xlarge"));
        assert!(msg.contains("on-demand"));
        assert!(msg.contains("pending 42m"));
    }

    #[test]
    fn empty_logs_message_spot_capacity() {
        let mut step = make_step("s");
        step.job_details = Some(JobDetails {
            job_type: JobType::Training,
            job_name: "job".to_string(),
            job_arn: None,
            secondary_status: Some("Starting".to_string()),
            status_message: Some("Preparing the instances for training".to_string()),
            managed_spot: Some(true),
            instance_type: Some("ml.g5.xlarge".to_string()),
            instance_count: Some(2),
        });
        let msg = step.empty_logs_message();
        assert!(msg.contains("2x ml.g5.xlarge"));
        assert!(msg.contains("spot"));
        assert!(msg.contains("starting"));
    }

    // --- StepInfo::start_time_str ---

    #[test]
    fn start_time_str_none() {
        let step = make_step("s");
        assert_eq!(step.start_time_str(), "--");
    }

    #[test]
    fn start_time_str_formats_hms_in_local_tz() {
        use chrono::{Local, TimeZone};
        let mut step = make_step("s");
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 14, 30, 45).unwrap();
        step.start_time = Some(dt);
        // Format through Local the same way fmt_local does — tz-independent.
        let expected = dt.with_timezone(&Local).format("%H:%M:%S").to_string();
        assert_eq!(step.start_time_str(), expected);
    }

    // --- StepInfo::instance_str ---

    fn make_job_details(instance_type: Option<&str>, instance_count: Option<i32>) -> JobDetails {
        JobDetails {
            job_type: JobType::Training,
            job_name: "job".to_string(),
            job_arn: None,
            secondary_status: None,
            status_message: None,
            managed_spot: None,
            instance_type: instance_type.map(|s| s.to_string()),
            instance_count,
        }
    }

    #[test]
    fn instance_str_no_job_details() {
        let step = make_step("s");
        assert_eq!(step.instance_str(), "--");
    }

    #[test]
    fn instance_str_missing_type() {
        let mut step = make_step("s");
        step.job_details = Some(make_job_details(None, Some(4)));
        assert_eq!(step.instance_str(), "--");
    }

    #[test]
    fn instance_str_single_instance() {
        let mut step = make_step("s");
        step.job_details = Some(make_job_details(Some("ml.m5.large"), Some(1)));
        assert_eq!(step.instance_str(), "ml.m5.large");
    }

    #[test]
    fn instance_str_single_instance_no_count() {
        let mut step = make_step("s");
        step.job_details = Some(make_job_details(Some("ml.m5.large"), None));
        assert_eq!(step.instance_str(), "ml.m5.large");
    }

    #[test]
    fn instance_str_multi_instance() {
        let mut step = make_step("s");
        step.job_details = Some(make_job_details(Some("ml.p3.8xl"), Some(4)));
        assert_eq!(step.instance_str(), "4x ml.p3.8xl");
    }
}
