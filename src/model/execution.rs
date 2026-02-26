use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    Executing,
    Succeeded,
    Failed,
    Stopped,
    Stopping,
    Unknown(String),
}

impl ExecutionStatus {
    pub fn from_str(s: &str) -> Self {
        match s {
            "Executing" => Self::Executing,
            "Succeeded" => Self::Succeeded,
            "Failed" => Self::Failed,
            "Stopped" => Self::Stopped,
            "Stopping" => Self::Stopping,
            other => Self::Unknown(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Executing => "Executing",
            Self::Succeeded => "Succeeded",
            Self::Failed => "Failed",
            Self::Stopped => "Stopped",
            Self::Stopping => "Stopping",
            Self::Unknown(s) => s.as_str(),
        }
    }

}

#[derive(Debug, Clone)]
pub struct ExecutionSummary {
    pub arn: String,
    pub display_name: Option<String>,
    pub status: ExecutionStatus,
    pub start_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct PipelineExecution {
    pub display_name: Option<String>,
    pub status: ExecutionStatus,
    pub created: Option<DateTime<Utc>>,
    pub last_modified: Option<DateTime<Utc>>,
}
