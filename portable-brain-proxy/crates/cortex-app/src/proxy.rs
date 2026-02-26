use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use adapter_rmvm::RmvmAdapter;
use anyhow::{Context, Result, anyhow};
use axum::extract::State;
use axum::http::header::{AUTHORIZATION, HeaderName};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use brain_store::BrainStore;
use chrono::Utc;
use planner_guard::{
    build_plan_only_prompt, deterministic_plan_from_manifest, extract_json_object, parse_plan_json,
    validate_plan_against_manifest,
};
use reqwest::Client;
use rmvm_grpc::{AppendEventRequest, GetManifestRequest};
use rmvm_proto::{ErrorCode, ExecuteRequest, ExecutionStatus, PublicManifest, RmvmPlan, Scope};
use serde_json::{Value as JsonValue, json};
use tokio::net::TcpListener;
use tracing::info;
use uuid::Uuid;

use crate::types::{
    AssistantMessage, ChatCompletionRequest, ChatCompletionResponse, Choice, CortexEnvelope,
    OpenAiError, OpenAiErrorResponse, Usage, message_content_as_text,
};

const HX_CORTEX_STATUS: &str = "x-cortex-status";
const HX_CORTEX_SEMANTIC_ROOT: &str = "x-cortex-semantic-root";
const HX_CORTEX_TRACE_ROOT: &str = "x-cortex-trace-root";
const HX_CORTEX_ERROR_CODE: &str = "x-cortex-error-code";
const HX_CORTEX_STALL_HANDLE: &str = "x-cortex-stall-handle";
const HX_CORTEX_STALL_AVAILABILITY: &str = "x-cortex-stall-availability";
const HX_CORTEX_PLAN_SOURCE: &str = "x-cortex-plan-source";
const HX_CORTEX_PLAN_HEADER: &str = "x-cortex-plan";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannerMode {
    Fallback,
    OpenAi,
    ByoHeader,
}

impl PlannerMode {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "fallback" => Ok(Self::Fallback),
            "openai" => Ok(Self::OpenAi),
            "byo" | "byo_header" | "byoheader" => Ok(Self::ByoHeader),
            other => Err(anyhow!(
                "unsupported planner mode '{other}', expected fallback|openai|byo"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Fallback => "fallback",
            Self::OpenAi => "openai",
            Self::ByoHeader => "byo_header",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlannerConfig {
    pub mode: PlannerMode,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub bind_addr: SocketAddr,
    pub endpoint: String,
    pub default_brain: Option<String>,
    pub brain_home: Option<PathBuf>,
    pub planner: PlannerConfig,
}

#[derive(Clone)]
struct AppState {
    endpoint: String,
    default_brain: Option<String>,
    brain_home: Option<PathBuf>,
    planner: PlannerConfig,
    planner_http: Client,
}

#[derive(Debug, Clone)]
struct RequestContext {
    subject: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: String,
    message: String,
    headers: Vec<(HeaderName, HeaderValue)>,
}

impl ApiError {
    fn bad_request(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: code.into(),
            message: message.into(),
            headers: Vec::new(),
        }
    }

    fn unauthorized(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: code.into(),
            message: message.into(),
            headers: Vec::new(),
        }
    }

    fn unavailable(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: code.into(),
            message: message.into(),
            headers: Vec::new(),
        }
    }

    fn bad_gateway(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: code.into(),
            message: message.into(),
            headers: Vec::new(),
        }
    }

    fn with_headers(mut self, headers: Vec<(HeaderName, HeaderValue)>) -> Self {
        self.headers = headers;
        self
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = Json(OpenAiErrorResponse {
            error: OpenAiError {
                message: self.message,
                error_type: "invalid_request_error".to_string(),
                code: self.code,
            },
        })
        .into_response();
        *response.status_mut() = self.status;
        for (name, value) in self.headers {
            response.headers_mut().insert(name, value);
        }
        response
    }
}

pub fn parse_addr(value: &str) -> Result<SocketAddr> {
    value
        .parse::<SocketAddr>()
        .with_context(|| format!("invalid socket address '{value}'"))
}

pub async fn serve(config: ProxyConfig) -> Result<()> {
    let listener = TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("failed to bind {}", config.bind_addr))?;
    serve_on_listener(listener, config, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
}

async fn serve_on_listener(
    listener: TcpListener,
    config: ProxyConfig,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<()> {
    let addr = listener.local_addr()?;
    let state = build_state(config)?;
    info!(
        "cortex proxy listening on http://{} (rmvm endpoint={}, planner_mode={})",
        addr,
        state.endpoint,
        state.planner.mode.as_str()
    );

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/chat/completions", post(chat_completions))
        .with_state(Arc::new(state));

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .context("proxy server failed")
}

fn build_state(config: ProxyConfig) -> Result<AppState> {
    let planner_http = Client::builder()
        .timeout(config.planner.timeout)
        .build()
        .context("failed to build planner HTTP client")?;
    Ok(AppState {
        endpoint: config.endpoint,
        default_brain: config.default_brain,
        brain_home: config.brain_home,
        planner: config.planner,
        planner_http,
    })
}

async fn healthz() -> &'static str {
    "ok"
}

async fn chat_completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<ChatCompletionRequest>,
) -> Response {
    match handle_chat_completion(state, headers, request).await {
        Ok(response) => response,
        Err(err) => err.into_response(),
    }
}

async fn handle_chat_completion(
    state: Arc<AppState>,
    headers: HeaderMap,
    request: ChatCompletionRequest,
) -> Result<Response, ApiError> {
    if request.stream.unwrap_or(false) {
        return Err(ApiError::bad_request(
            "stream_not_supported",
            "stream=true is not supported in proxy v0",
        ));
    }

    let user_message = extract_user_message(&request)
        .ok_or_else(|| ApiError::bad_request("missing_user_message", "no user message found"))?;
    let ctx = resolve_context(&state, &headers, &request)?;

    let request_id = format!("req-{}", Uuid::new_v4().simple());
    let adapter = RmvmAdapter::new(state.endpoint.clone());

    adapter
        .append_event(AppendEventRequest {
            request_id: request_id.clone(),
            subject: ctx.subject.clone(),
            text: user_message.clone(),
            scope: Scope::Global as i32,
        })
        .await
        .map_err(|e| ApiError::bad_gateway("append_event_failed", e.to_string()))?;

    let manifest = adapter
        .get_manifest(GetManifestRequest {
            request_id: request_id.clone(),
        })
        .await
        .map_err(|e| ApiError::bad_gateway("get_manifest_failed", e.to_string()))?
        .manifest
        .ok_or_else(|| ApiError::bad_gateway("manifest_missing", "rmvm returned no manifest"))?;

    let plan_prompt = build_plan_only_prompt(&user_message, &manifest);
    let (plan, plan_source) = resolve_plan(
        &state,
        &headers,
        &plan_prompt,
        &manifest,
        &request_id,
        &ctx.subject,
    )
    .await?;

    validate_plan_against_manifest(&plan, &manifest)
        .map_err(|e| ApiError::bad_request("invalid_plan", e.to_string()))?;

    let execute = adapter
        .execute(ExecuteRequest {
            manifest: Some(manifest),
            plan: Some(plan),
        })
        .await
        .map_err(|e| ApiError::bad_gateway("execute_failed", e.to_string()))?;

    let headers_out = cortex_headers(&execute, &plan_source);
    map_execute_response(execute, request, plan_prompt, plan_source, headers_out)
}

fn resolve_context(
    state: &AppState,
    headers: &HeaderMap,
    request: &ChatCompletionRequest,
) -> Result<RequestContext, ApiError> {
    let store = BrainStore::new(state.brain_home.clone())
        .map_err(|e| ApiError::bad_gateway("brain_store_init_failed", e.to_string()))?;

    let maybe_api_key = parse_bearer(headers)?;
    if let Some(api_key) = maybe_api_key {
        let mapping = store
            .resolve_api_key(&api_key)
            .map_err(|e| ApiError::bad_gateway("auth_lookup_failed", e.to_string()))?
            .ok_or_else(|| ApiError::unauthorized("auth_failed", "API key is not mapped"))?;
        return Ok(RequestContext {
            subject: mapping.subject,
        });
    }

    let _ = store
        .resolve_brain_or_active(state.default_brain.as_deref())
        .map_err(|_| {
            ApiError::unauthorized(
                "auth_required",
                "missing bearer token and no default/active brain configured",
            )
        })?;

    Ok(RequestContext {
        subject: request
            .user
            .clone()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "user:local".to_string()),
    })
}

fn parse_bearer(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
    let Some(value) = headers.get(AUTHORIZATION) else {
        return Ok(None);
    };
    let raw = value.to_str().map_err(|_| {
        ApiError::unauthorized("invalid_auth_header", "invalid Authorization header")
    })?;
    let Some(token) = raw.strip_prefix("Bearer ") else {
        return Err(ApiError::unauthorized(
            "invalid_auth_header",
            "Authorization must use Bearer token",
        ));
    };
    if token.trim().is_empty() {
        return Err(ApiError::unauthorized(
            "invalid_auth_header",
            "Bearer token is empty",
        ));
    }
    Ok(Some(token.trim().to_string()))
}

fn extract_user_message(request: &ChatCompletionRequest) -> Option<String> {
    request
        .messages
        .iter()
        .rev()
        .find(|m| m.role.eq_ignore_ascii_case("user"))
        .and_then(|m| message_content_as_text(&m.content))
}

async fn resolve_plan(
    state: &AppState,
    headers: &HeaderMap,
    plan_prompt: &str,
    manifest: &PublicManifest,
    request_id: &str,
    subject: &str,
) -> Result<(RmvmPlan, String), ApiError> {
    if let Some(header) = headers.get(HX_CORTEX_PLAN_HEADER) {
        let plan = parse_byo_plan(header, request_id)?;
        return Ok((plan, PlannerMode::ByoHeader.as_str().to_string()));
    }

    match state.planner.mode {
        PlannerMode::ByoHeader => Err(ApiError::bad_request(
            "plan_header_required",
            "planner mode BYO requires X-Cortex-Plan header",
        )),
        PlannerMode::Fallback => deterministic_plan_from_manifest(request_id, subject, manifest)
            .map(|plan| (plan, PlannerMode::Fallback.as_str().to_string()))
            .map_err(|e| ApiError::bad_request("fallback_plan_failed", e.to_string())),
        PlannerMode::OpenAi => {
            let plan = request_openai_plan(state, plan_prompt, manifest, request_id).await?;
            Ok((plan, PlannerMode::OpenAi.as_str().to_string()))
        }
    }
}

fn parse_byo_plan(header: &HeaderValue, request_id: &str) -> Result<RmvmPlan, ApiError> {
    let raw = header
        .to_str()
        .map_err(|_| ApiError::bad_request("invalid_plan_header", "X-Cortex-Plan must be UTF-8"))?;
    let bytes = B64.decode(raw).map_err(|_| {
        ApiError::bad_request("invalid_plan_header", "X-Cortex-Plan must be base64")
    })?;
    let text = String::from_utf8(bytes)
        .map_err(|_| ApiError::bad_request("invalid_plan_header", "decoded plan is not UTF-8"))?;
    let plan_json = extract_json_object(&text)
        .map_err(|e| ApiError::bad_request("invalid_plan_json", e.to_string()))?;
    parse_plan_json(&plan_json, request_id)
        .map_err(|e| ApiError::bad_request("invalid_plan_json", e.to_string()))
}

async fn request_openai_plan(
    state: &AppState,
    plan_prompt: &str,
    manifest: &PublicManifest,
    request_id: &str,
) -> Result<RmvmPlan, ApiError> {
    let api_key = state.planner.api_key.clone().ok_or_else(|| {
        ApiError::bad_gateway(
            "planner_auth_missing",
            "openai planner mode requires CORTEX_PLANNER_API_KEY or OPENAI_API_KEY",
        )
    })?;

    let url = format!(
        "{}/chat/completions",
        state.planner.base_url.trim_end_matches('/')
    );
    let payload = json!({
        "model": state.planner.model,
        "temperature": 0,
        "messages": [
            {"role":"system","content":"Return only JSON matching the RMVMPlan schema. No markdown and no prose."},
            {"role":"user","content": plan_prompt}
        ]
    });

    let resp = state
        .planner_http
        .post(url)
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await
        .map_err(|e| ApiError::bad_gateway("planner_http_failed", e.to_string()))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| ApiError::bad_gateway("planner_http_failed", e.to_string()))?;
    if !status.is_success() {
        return Err(ApiError::bad_gateway(
            "planner_http_failed",
            format!("planner returned HTTP {}: {}", status.as_u16(), body),
        ));
    }

    let root: JsonValue = serde_json::from_str(&body)
        .map_err(|e| ApiError::bad_gateway("planner_decode_failed", e.to_string()))?;
    let content = root
        .pointer("/choices/0/message/content")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| {
            ApiError::bad_gateway(
                "planner_decode_failed",
                "planner response missing choices[0].message.content",
            )
        })?;
    let plan_json = extract_json_object(content)
        .map_err(|e| ApiError::bad_request("planner_output_invalid", e.to_string()))?;
    let plan = parse_plan_json(&plan_json, request_id)
        .map_err(|e| ApiError::bad_request("planner_output_invalid", e.to_string()))?;
    validate_plan_against_manifest(&plan, manifest)
        .map_err(|e| ApiError::bad_request("invalid_plan", e.to_string()))?;
    Ok(plan)
}

fn map_execute_response(
    execute: rmvm_proto::ExecuteResponse,
    request: ChatCompletionRequest,
    plan_prompt: String,
    plan_source: String,
    headers_out: Vec<(HeaderName, HeaderValue)>,
) -> Result<Response, ApiError> {
    let status = ExecutionStatus::try_from(execute.status).unwrap_or(ExecutionStatus::Unspecified);
    match status {
        ExecutionStatus::Ok => {
            let verified_blocks = execute
                .rendered
                .as_ref()
                .map(|r| r.verified_blocks.clone())
                .unwrap_or_default();
            let content = if verified_blocks.is_empty() {
                "No verified output.".to_string()
            } else {
                verified_blocks.join("\n\n")
            };

            let model = request
                .model
                .unwrap_or_else(|| "cortex-rmvm-proxy".to_string());
            let response = ChatCompletionResponse {
                id: format!("chatcmpl-{}", Uuid::new_v4().simple()),
                object: "chat.completion".to_string(),
                created: Utc::now().timestamp(),
                model,
                choices: vec![Choice {
                    index: 0,
                    message: AssistantMessage {
                        role: "assistant".to_string(),
                        content,
                    },
                    finish_reason: "stop".to_string(),
                }],
                usage: Usage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                },
                cortex: CortexEnvelope {
                    status: status.as_str_name().to_string(),
                    semantic_root: execute.proof.as_ref().map(|p| p.semantic_root.clone()),
                    trace_root: execute.proof.as_ref().map(|p| p.trace_root.clone()),
                    error_code: execute.error.as_ref().map(error_code_name),
                    plan_prompt: Some(plan_prompt),
                    plan_source: Some(plan_source),
                },
            };
            let mut out = Json(response).into_response();
            for (name, value) in headers_out {
                out.headers_mut().insert(name, value);
            }
            Ok(out)
        }
        ExecutionStatus::Rejected => Err(ApiError::bad_request(
            execute
                .error
                .as_ref()
                .map(error_code_name)
                .unwrap_or_else(|| "rejected".to_string()),
            execute
                .error
                .as_ref()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "request rejected by RMVM".to_string()),
        )
        .with_headers(headers_out)),
        ExecutionStatus::Stall => Err(ApiError::unavailable(
            execute
                .error
                .as_ref()
                .map(error_code_name)
                .unwrap_or_else(|| "stall".to_string()),
            execute
                .error
                .as_ref()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "execution stalled; dependency not ready".to_string()),
        )
        .with_headers(headers_out)),
        ExecutionStatus::AuthDenied => Err(ApiError {
            status: StatusCode::FORBIDDEN,
            code: execute
                .error
                .as_ref()
                .map(error_code_name)
                .unwrap_or_else(|| "auth_denied".to_string()),
            message: execute
                .error
                .as_ref()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "auth denied".to_string()),
            headers: headers_out,
        }),
        ExecutionStatus::RangeExceeded => Err(ApiError {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: execute
                .error
                .as_ref()
                .map(error_code_name)
                .unwrap_or_else(|| "range_exceeded".to_string()),
            message: execute
                .error
                .as_ref()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "range exceeded".to_string()),
            headers: headers_out,
        }),
        ExecutionStatus::Unspecified => Err(ApiError {
            status: StatusCode::BAD_GATEWAY,
            code: execute
                .error
                .as_ref()
                .map(error_code_name)
                .unwrap_or_else(|| "unknown_status".to_string()),
            message: "RMVM returned unspecified status".to_string(),
            headers: headers_out,
        }),
    }
}

fn cortex_headers(
    execute: &rmvm_proto::ExecuteResponse,
    plan_source: &str,
) -> Vec<(HeaderName, HeaderValue)> {
    let mut headers = Vec::new();
    push_header(
        &mut headers,
        HX_CORTEX_STATUS,
        ExecutionStatus::try_from(execute.status)
            .unwrap_or(ExecutionStatus::Unspecified)
            .as_str_name(),
    );
    push_header(&mut headers, HX_CORTEX_PLAN_SOURCE, plan_source);
    if let Some(proof) = execute.proof.as_ref() {
        push_header(&mut headers, HX_CORTEX_SEMANTIC_ROOT, &proof.semantic_root);
        push_header(&mut headers, HX_CORTEX_TRACE_ROOT, &proof.trace_root);
    }
    if let Some(err) = execute.error.as_ref() {
        push_header(&mut headers, HX_CORTEX_ERROR_CODE, &error_code_name(err));
    }
    if let Some(stall) = execute.stall.as_ref() {
        push_header(&mut headers, HX_CORTEX_STALL_HANDLE, &stall.handle_ref);
        push_header(
            &mut headers,
            HX_CORTEX_STALL_AVAILABILITY,
            rmvm_proto::HandleAvailability::try_from(stall.availability)
                .unwrap_or(rmvm_proto::HandleAvailability::Unspecified)
                .as_str_name(),
        );
    }
    headers
}

fn push_header(headers: &mut Vec<(HeaderName, HeaderValue)>, name: &'static str, value: &str) {
    if let (Ok(name), Ok(value)) = (
        HeaderName::from_bytes(name.as_bytes()),
        HeaderValue::from_str(value),
    ) {
        headers.push((name, value));
    }
}

fn error_code_name(err: &rmvm_proto::ExecutionError) -> String {
    ErrorCode::try_from(err.code)
        .unwrap_or(ErrorCode::Unspecified)
        .as_str_name()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use axum::routing::post;
    use brain_store::{BrainStore, CreateBrainRequest};
    use rmvm_grpc::{
        AppendEventResponse, ForgetRequest, ForgetResponse, GetManifestResponse, RmvmExecutor,
        RmvmExecutorServer,
    };
    use rmvm_proto::cortex::rmvm::v3_1::value::V;
    use rmvm_proto::{
        AssertionMerkleProof, ErrorCode, ExecuteResponse, ExecutionError, HandleAvailability,
        HandleMeta, HandleRef, PlanBudget, RenderedOutput, Scope, StallInfo, TrustTier, Value,
        VerifiedAssertion,
    };
    use tokio::sync::oneshot;
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::{Request, Response, Status};

    #[derive(Clone, Copy)]
    enum MockMode {
        Ok,
        Rejected,
        Stall,
    }

    #[derive(Clone)]
    struct MockRmvmService {
        mode: MockMode,
    }

    #[tonic::async_trait]
    impl RmvmExecutor for MockRmvmService {
        async fn append_event(
            &self,
            _request: Request<AppendEventRequest>,
        ) -> Result<Response<AppendEventResponse>, Status> {
            Ok(Response::new(AppendEventResponse {
                event_id: "evt-1".to_string(),
                handle_refs: vec!["H1".to_string()],
            }))
        }

        async fn get_manifest(
            &self,
            request: Request<GetManifestRequest>,
        ) -> Result<Response<GetManifestResponse>, Status> {
            let req = request.into_inner();
            Ok(Response::new(GetManifestResponse {
                manifest: Some(PublicManifest {
                    request_id: req.request_id,
                    handles: vec![HandleRef {
                        r#ref: "H1".to_string(),
                        type_id: "normative.preference".to_string(),
                        availability: HandleAvailability::Ready as i32,
                        meta: Some(HandleMeta {
                            subject: "user:local".to_string(),
                            predicate_label: "prefers_beverage".to_string(),
                            trust_tier: TrustTier::Tier3Confirmed as i32,
                            taint: vec![],
                            temporal: None,
                            scope: Scope::Global as i32,
                        }),
                        signature_summary: "prefers_beverage=tea".to_string(),
                        conflict_group_id: "c1".to_string(),
                    }],
                    selectors: Vec::new(),
                    context: Vec::new(),
                    budget: Some(PlanBudget {
                        max_ops: 8,
                        max_join_depth: 2,
                        max_fanout: 8,
                        max_total_cost: 8.0,
                    }),
                }),
            }))
        }

        async fn execute(
            &self,
            _request: Request<ExecuteRequest>,
        ) -> Result<Response<ExecuteResponse>, Status> {
            let response = match self.mode {
                MockMode::Ok => ExecuteResponse {
                    status: ExecutionStatus::Ok as i32,
                    assertions: vec![VerifiedAssertion {
                        assertion_type: rmvm_proto::AssertionType::AssertWorldFact as i32,
                        fields: BTreeMap::from([(
                            "subject".to_string(),
                            Value {
                                v: Some(V::S("user:local".to_string())),
                            },
                        )]),
                        citations: Vec::new(),
                    }],
                    proof: Some(AssertionMerkleProof {
                        semantic_root: "sem-root-ok".to_string(),
                        trace_root: "trace-root-ok".to_string(),
                        inclusion: Vec::new(),
                    }),
                    rendered: Some(RenderedOutput {
                        verified_blocks: vec!["Verified: user prefers tea.".to_string()],
                        narrative_blocks: Vec::new(),
                    }),
                    stall: None,
                    error: None,
                },
                MockMode::Rejected => ExecuteResponse {
                    status: ExecutionStatus::Rejected as i32,
                    assertions: Vec::new(),
                    proof: Some(AssertionMerkleProof {
                        semantic_root: "sem-root-rejected".to_string(),
                        trace_root: "trace-root-rejected".to_string(),
                        inclusion: Vec::new(),
                    }),
                    rendered: Some(RenderedOutput {
                        verified_blocks: Vec::new(),
                        narrative_blocks: Vec::new(),
                    }),
                    stall: None,
                    error: Some(ExecutionError {
                        code: ErrorCode::TypeMismatch as i32,
                        message: "type mismatch".to_string(),
                        hints: Vec::new(),
                    }),
                },
                MockMode::Stall => ExecuteResponse {
                    status: ExecutionStatus::Stall as i32,
                    assertions: Vec::new(),
                    proof: Some(AssertionMerkleProof {
                        semantic_root: "sem-root-stall".to_string(),
                        trace_root: "trace-root-stall".to_string(),
                        inclusion: Vec::new(),
                    }),
                    rendered: Some(RenderedOutput {
                        verified_blocks: Vec::new(),
                        narrative_blocks: Vec::new(),
                    }),
                    stall: Some(StallInfo {
                        handle_ref: "H1".to_string(),
                        availability: HandleAvailability::ArchivalPending as i32,
                        estimated_ready_at: None,
                        retrieval_ticket: "ticket-1".to_string(),
                    }),
                    error: Some(ExecutionError {
                        code: ErrorCode::HandleNotReady as i32,
                        message: "handle not ready".to_string(),
                        hints: Vec::new(),
                    }),
                },
            };
            Ok(Response::new(response))
        }

        async fn forget(
            &self,
            _request: Request<ForgetRequest>,
        ) -> Result<Response<ForgetResponse>, Status> {
            Ok(Response::new(ForgetResponse {
                status: ExecutionStatus::Ok as i32,
                assertions: Vec::new(),
                rendered: Some(RenderedOutput {
                    verified_blocks: Vec::new(),
                    narrative_blocks: Vec::new(),
                }),
                error: None,
            }))
        }
    }

    async fn spawn_mock_rmvm(mode: MockMode) -> (String, oneshot::Sender<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let incoming = TcpListenerStream::new(listener);
        let (tx, rx) = oneshot::channel::<()>();
        let svc = MockRmvmService { mode };
        tokio::spawn(async move {
            let _ = tonic::transport::Server::builder()
                .add_service(RmvmExecutorServer::new(svc))
                .serve_with_incoming_shutdown(incoming, async {
                    let _ = rx.await;
                })
                .await;
        });
        (format!("grpc://{}", addr), tx)
    }

    async fn spawn_mock_planner(plan_json: String) -> (String, oneshot::Sender<()>) {
        let app = Router::new().route(
            "/chat/completions",
            post(move |Json(_req): Json<JsonValue>| {
                let plan_json = plan_json.clone();
                async move {
                    Json(json!({
                        "id":"pln_1",
                        "object":"chat.completion",
                        "created": 0,
                        "choices":[{"index":0,"message":{"role":"assistant","content": plan_json},"finish_reason":"stop"}]
                    }))
                }
            }),
        );

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await;
        });
        (format!("http://{}", addr), tx)
    }

    fn sample_byo_plan_b64() -> String {
        B64.encode(
            r#"{
              "requestId":"req-e2e",
              "steps":[
                {"out":"r0","op":{"kind":"fetch","handleRef":"H1"}},
                {"out":"r1","op":{"kind":"project","inReg":"r0","fieldPaths":["meta.subject"]}}
              ],
              "outputs":["r1"]
            }"#,
        )
    }

    async fn start_proxy(
        home: PathBuf,
        endpoint: String,
        planner: PlannerConfig,
    ) -> (String, oneshot::Sender<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = serve_on_listener(
                listener,
                ProxyConfig {
                    bind_addr: addr,
                    endpoint,
                    default_brain: None,
                    brain_home: Some(home),
                    planner,
                },
                async {
                    let _ = rx.await;
                },
            )
            .await;
        });
        (format!("http://{}", addr), tx)
    }

    fn setup_store(home: &PathBuf) -> (String, String) {
        unsafe {
            std::env::set_var("TEST_BRAIN_SECRET_PROXY", "test-secret-proxy");
        }
        let store = BrainStore::new(Some(home.clone())).unwrap();
        let brain = store
            .create_brain(CreateBrainRequest {
                name: "proxy-test".to_string(),
                tenant_id: "local".to_string(),
                passphrase_env: Some("TEST_BRAIN_SECRET_PROXY".to_string()),
            })
            .unwrap();
        let api_key = "proxy-test-key".to_string();
        store
            .map_api_key(&api_key, "local", &brain.brain_id, "user:local")
            .unwrap();
        (brain.brain_id, api_key)
    }

    async fn send_chat(
        base_url: &str,
        api_key: &str,
        extra_headers: Vec<(&str, String)>,
    ) -> reqwest::Response {
        let client = reqwest::Client::new();
        let mut req = client
            .post(format!("{base_url}/v1/chat/completions"))
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .body(
                r#"{"model":"gpt-4o-mini","messages":[{"role":"user","content":"I prefer tea."}]}"#,
            );
        for (name, value) in extra_headers {
            req = req.header(name, value);
        }
        req.send().await.unwrap()
    }

    #[tokio::test]
    async fn e2e_status_mapping_and_headers_in_process() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().to_path_buf();
        let (_brain_id, api_key) = setup_store(&home);

        for (mode, expected_status, expected_http) in [
            (MockMode::Ok, "OK", StatusCode::OK),
            (MockMode::Rejected, "REJECTED", StatusCode::BAD_REQUEST),
            (MockMode::Stall, "STALL", StatusCode::SERVICE_UNAVAILABLE),
        ] {
            let (grpc_endpoint, stop_grpc) = spawn_mock_rmvm(mode).await;
            let (proxy_base, stop_proxy) = start_proxy(
                home.clone(),
                grpc_endpoint,
                PlannerConfig {
                    mode: PlannerMode::ByoHeader,
                    base_url: "http://unused".to_string(),
                    model: "unused".to_string(),
                    api_key: None,
                    timeout: Duration::from_secs(5),
                },
            )
            .await;

            let resp = send_chat(
                &proxy_base,
                &api_key,
                vec![(HX_CORTEX_PLAN_HEADER, sample_byo_plan_b64())],
            )
            .await;
            assert_eq!(resp.status(), expected_http);

            let headers = resp.headers().clone();
            assert_eq!(
                headers
                    .get(HX_CORTEX_STATUS)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default(),
                expected_status
            );
            assert_eq!(
                headers
                    .get(HX_CORTEX_PLAN_SOURCE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default(),
                "byo_header"
            );

            let body: JsonValue = resp.json().await.unwrap();
            if expected_status == "OK" {
                assert_eq!(
                    body.get("object").and_then(|v| v.as_str()),
                    Some("chat.completion")
                );
                let content = body
                    .pointer("/choices/0/message/content")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                assert!(content.contains("Verified"));
                assert_eq!(
                    body.pointer("/cortex/status").and_then(|v| v.as_str()),
                    Some("OK")
                );
            } else {
                assert!(body.get("error").is_some());
                if expected_status == "STALL" {
                    assert!(headers.get(HX_CORTEX_STALL_HANDLE).is_some());
                }
            }

            let _ = stop_proxy.send(());
            let _ = stop_grpc.send(());
        }
    }

    #[tokio::test]
    async fn e2e_openai_planner_mode_without_byo_header() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().to_path_buf();
        let (_brain_id, api_key) = setup_store(&home);
        let (grpc_endpoint, stop_grpc) = spawn_mock_rmvm(MockMode::Ok).await;
        let (planner_url, stop_planner) = spawn_mock_planner(
            r#"{
              "requestId":"req-openai",
              "steps":[
                {"out":"r0","op":{"kind":"fetch","handleRef":"H1"}},
                {"out":"r1","op":{"kind":"project","inReg":"r0","fieldPaths":["meta.subject"]}}
              ],
              "outputs":["r1"]
            }"#
            .to_string(),
        )
        .await;

        let (proxy_base, stop_proxy) = start_proxy(
            home.clone(),
            grpc_endpoint,
            PlannerConfig {
                mode: PlannerMode::OpenAi,
                base_url: planner_url,
                model: "planner-model".to_string(),
                api_key: Some("planner-secret".to_string()),
                timeout: Duration::from_secs(5),
            },
        )
        .await;

        let resp = send_chat(&proxy_base, &api_key, vec![]).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let headers = resp.headers().clone();
        assert_eq!(
            headers
                .get(HX_CORTEX_PLAN_SOURCE)
                .and_then(|v| v.to_str().ok()),
            Some("openai")
        );
        let body: JsonValue = resp.json().await.unwrap();
        assert_eq!(
            body.pointer("/cortex/plan_source").and_then(|v| v.as_str()),
            Some("openai")
        );

        let _ = stop_proxy.send(());
        let _ = stop_planner.send(());
        let _ = stop_grpc.send(());
    }
}
