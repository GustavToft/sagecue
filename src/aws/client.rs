use anyhow::Result;
use aws_sdk_cloudwatchlogs::Client as CloudWatchLogsClient;
use aws_sdk_sagemaker::Client as SageMakerClient;
use aws_sdk_sagemakermetrics::Client as SageMakerMetricsClient;

#[derive(Clone)]
pub struct AwsClients {
    pub sagemaker: SageMakerClient,
    pub cloudwatch_logs: CloudWatchLogsClient,
    pub sagemaker_metrics: SageMakerMetricsClient,
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
            sagemaker_metrics: SageMakerMetricsClient::new(&config),
        })
    }
}
