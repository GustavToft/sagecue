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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_known_variants() {
        assert_eq!(ExecutionStatus::from_str("Executing"), ExecutionStatus::Executing);
        assert_eq!(ExecutionStatus::from_str("Succeeded"), ExecutionStatus::Succeeded);
        assert_eq!(ExecutionStatus::from_str("Failed"), ExecutionStatus::Failed);
        assert_eq!(ExecutionStatus::from_str("Stopped"), ExecutionStatus::Stopped);
        assert_eq!(ExecutionStatus::from_str("Stopping"), ExecutionStatus::Stopping);
    }

    #[test]
    fn from_str_unknown_fallback() {
        assert_eq!(
            ExecutionStatus::from_str("Banana"),
            ExecutionStatus::Unknown("Banana".to_string())
        );
    }

    #[test]
    fn as_str_roundtrip() {
        for variant in ["Executing", "Succeeded", "Failed", "Stopped", "Stopping"] {
            assert_eq!(ExecutionStatus::from_str(variant).as_str(), variant);
        }
    }

    #[test]
    fn unknown_as_str_preserves_value() {
        let status = ExecutionStatus::from_str("CustomStatus");
        assert_eq!(status.as_str(), "CustomStatus");
    }
}
