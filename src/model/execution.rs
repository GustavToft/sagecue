use std::collections::BTreeMap;

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

impl std::str::FromStr for ExecutionStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "Executing" => Self::Executing,
            "Succeeded" => Self::Succeeded,
            "Failed" => Self::Failed,
            "Stopped" => Self::Stopped,
            "Stopping" => Self::Stopping,
            other => Self::Unknown(other.to_string()),
        })
    }
}

impl ExecutionStatus {
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
    pub failure_reason: Option<String>,
    pub parameters: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> ExecutionStatus {
        s.parse().unwrap()
    }

    #[test]
    fn from_str_known_variants() {
        assert_eq!(parse("Executing"), ExecutionStatus::Executing);
        assert_eq!(parse("Succeeded"), ExecutionStatus::Succeeded);
        assert_eq!(parse("Failed"), ExecutionStatus::Failed);
        assert_eq!(parse("Stopped"), ExecutionStatus::Stopped);
        assert_eq!(parse("Stopping"), ExecutionStatus::Stopping);
    }

    #[test]
    fn from_str_unknown_fallback() {
        assert_eq!(
            parse("Banana"),
            ExecutionStatus::Unknown("Banana".to_string())
        );
    }

    #[test]
    fn as_str_roundtrip() {
        for variant in ["Executing", "Succeeded", "Failed", "Stopped", "Stopping"] {
            assert_eq!(parse(variant).as_str(), variant);
        }
    }

    #[test]
    fn unknown_as_str_preserves_value() {
        let status = parse("CustomStatus");
        assert_eq!(status.as_str(), "CustomStatus");
    }
}
