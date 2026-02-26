use anyhow::{Context, Result};
use rmvm_grpc::{
    AppendEventRequest, ForgetRequest, ForgetResponse, GetManifestRequest, GetManifestResponse,
    RmvmExecutorClient,
};
use rmvm_proto::{ExecuteRequest, ExecuteResponse};
use tonic::transport::Channel;

#[derive(Debug, Clone)]
pub struct RmvmAdapter {
    endpoint: String,
}

impl RmvmAdapter {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: normalize_endpoint(&endpoint.into()),
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub async fn append_event(
        &self,
        req: AppendEventRequest,
    ) -> Result<rmvm_grpc::AppendEventResponse> {
        let mut client = self.client().await?;
        let resp = client
            .append_event(req)
            .await
            .context("append_event RPC failed")?
            .into_inner();
        Ok(resp)
    }

    pub async fn get_manifest(&self, req: GetManifestRequest) -> Result<GetManifestResponse> {
        let mut client = self.client().await?;
        let resp = client
            .get_manifest(req)
            .await
            .context("get_manifest RPC failed")?
            .into_inner();
        Ok(resp)
    }

    pub async fn execute(&self, req: ExecuteRequest) -> Result<ExecuteResponse> {
        let mut client = self.client().await?;
        let resp = client
            .execute(req)
            .await
            .context("execute RPC failed")?
            .into_inner();
        Ok(resp)
    }

    pub async fn forget(&self, req: ForgetRequest) -> Result<ForgetResponse> {
        let mut client = self.client().await?;
        let resp = client
            .forget(req)
            .await
            .context("forget RPC failed")?
            .into_inner();
        Ok(resp)
    }

    async fn client(&self) -> Result<RmvmExecutorClient<Channel>> {
        RmvmExecutorClient::connect(self.endpoint.clone())
            .await
            .with_context(|| format!("failed to connect to RMVM endpoint {}", self.endpoint))
    }
}

fn normalize_endpoint(input: &str) -> String {
    if let Some(rest) = input.strip_prefix("grpc://") {
        format!("http://{rest}")
    } else if input.starts_with("http://") || input.starts_with("https://") {
        input.to_string()
    } else {
        format!("http://{input}")
    }
}
