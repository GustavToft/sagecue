use chrono::{DateTime, Utc};

pub const PIPELINE_STEPS: [&str; 4] = [
    "ValidateDataset",
    "TrainYOLOv8",
    "CompileForHailo",
    "ReorganizeFiles",
];

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
    pub status: StepStatus,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub failure_reason: Option<String>,
    pub job_details: Option<JobDetails>,
}

impl StepInfo {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
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
        let dur = end - start;
        let secs = dur.num_seconds();
        if secs < 0 {
            return "--".to_string();
        }
        let mins = secs / 60;
        let remaining_secs = secs % 60;
        if mins > 0 {
            format!("{}m {:02}s", mins, remaining_secs)
        } else {
            format!("{}s", remaining_secs)
        }
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
