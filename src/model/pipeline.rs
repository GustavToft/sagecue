use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct PipelineSummary {
    pub name: String,
    pub description: Option<String>,
    pub last_execution_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct PipelineParameter {
    pub name: String,
    pub type_name: String,
    /// The parameter's true default from the pipeline definition.
    /// `None` means the parameter is required (must be supplied on every start).
    pub default_value: Option<String>,
    /// What the editor should pre-populate: the default if set, otherwise the
    /// value used in the most recent execution, otherwise empty.
    pub initial_value: String,
}

impl PipelineParameter {
    pub fn is_required(&self) -> bool {
        self.default_value.is_none()
    }
}
