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
    pub instance_type: Option<String>,
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

    pub fn detail_str(&self) -> String {
        if let Some(ref details) = self.job_details {
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
            instance_type: None,
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
}
