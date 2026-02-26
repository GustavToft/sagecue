use anyhow::Result;
use aws_sdk_cloudwatchlogs::Client as CloudWatchLogsClient;
use aws_sdk_sagemaker::Client as SageMakerClient;

#[derive(Clone)]
pub struct AwsClients {
    pub sagemaker: SageMakerClient,
    pub cloudwatch_logs: CloudWatchLogsClient,
}

impl AwsClients {
    pub async fn from_env(region: &str) -> Result<Self> {
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(region.to_string()))
            .load()
            .await;

        Ok(Self {
            sagemaker: SageMakerClient::new(&config),
            cloudwatch_logs: CloudWatchLogsClient::new(&config),
        })
    }
}
