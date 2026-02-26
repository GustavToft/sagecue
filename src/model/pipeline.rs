use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct PipelineSummary {
    pub name: String,
    pub description: Option<String>,
    pub last_execution_time: Option<DateTime<Utc>>,
}
