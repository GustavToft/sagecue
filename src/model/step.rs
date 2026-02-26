use chrono::{DateTime, Utc};

use super::format::format_duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepStatus {
    NotStarted,
    Executing,
    Succeeded,
    Failed,
    Stopped,
    Unknown(String),
}

impl StepStatus {
    pub fn from_str(s: &str) -> Self {
        match s {
            "Executing" => Self::Executing,
            "Succeeded" => Self::Succeeded,
            "Failed" => Self::Failed,
            "Stopped" => Self::Stopped,
            other => Self::Unknown(other.to_string()),
        }
    }

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
    pub fn has_job(&self) -> bool {
        matches!(self, Self::Training | Self::Processing | Self::Transform)
    }

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
    pub job_arn: String,
    pub job_name: String,
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
    pub fn new(name: &str, step_type: StepType) -> Self {
        Self {
            name: name.to_string(),
            step_type,
            status: StepStatus::NotStarted,
            start_time: None,
            end_time: None,
            failure_reason: None,
            job_details: None,
        }
    }

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
            Some(t) => t.format("%H:%M:%S").to_string(),
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
