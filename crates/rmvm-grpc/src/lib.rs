pub mod grpc {
    tonic::include_proto!("cortex.rmvm.v3_1");
}

pub mod fixture_data;

use std::collections::BTreeMap;
use std::sync::Arc;

use rmvm_kernel::{ExecuteOptions, execute};
use rmvm_proto::cortex::rmvm::v3_1::value::V;
use rmvm_proto::{
    CanonicalCitation, ExecuteRequest, ExecuteResponse, ExecutionStatus, HandleAvailability,
    HandleMeta, RenderedOutput, Scope, SelectorRef, SelectorReturn, TemporalBound, TrustTier, Value,
    VerifiedAssertion,
};
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};

pub use grpc::rmvm_executor_client::RmvmExecutorClient;
pub use grpc::rmvm_executor_server::{RmvmExecutor, RmvmExecutorServer};
pub use grpc::{
    AppendEventRequest, AppendEventResponse, ForgetRequest, ForgetResponse, GetManifestRequest,
    GetManifestResponse,
};

#[derive(Debug, Default)]
struct ServiceState {
    next_event_id: u64,
    next_handle_id: u64,
    handles: BTreeMap<String, rmvm_proto::HandleRef>,
}

#[derive(Debug, Clone, Default)]
pub struct GrpcKernelService {
    options: ExecuteOptions,
    state: Arc<Mutex<ServiceState>>,
}

impl GrpcKernelService {
    pub fn new(options: ExecuteOptions) -> Self {
        Self {
            options,
            state: Arc::new(Mutex::new(ServiceState::default())),
        }
    }
}

#[tonic::async_trait]
impl RmvmExecutor for GrpcKernelService {
    async fn append_event(
        &self,
        request: Request<AppendEventRequest>,
    ) -> Result<Response<AppendEventResponse>, Status> {
        let req = request.into_inner();
        let mut state = self.state.lock().await;
        state.next_event_id += 1;
        let event_id = format!("EVT{}", state.next_event_id);

        let subject = if req.subject.trim().is_empty() {
            "user:unknown".to_string()
        } else {
            req.subject.clone()
        };
        let mut handle_refs = Vec::new();
        if let Some((predicate_label, preference)) = parse_preference(&req.text) {
            state.next_handle_id += 1;
            let handle_ref = format!("H{}", state.next_handle_id);
            let scope = Scope::try_from(req.scope).unwrap_or(Scope::Global);
            let handle = rmvm_proto::HandleRef {
                r#ref: handle_ref.clone(),
                type_id: "normative.preference".to_string(),
                availability: HandleAvailability::Ready as i32,
                meta: Some(HandleMeta {
                    subject: subject.clone(),
                    predicate_label: predicate_label.to_string(),
                    trust_tier: TrustTier::Tier3Confirmed as i32,
                    taint: vec![rmvm_proto::TaintClass::TaintUser as i32],
                    temporal: Some(TemporalBound {
                        valid_from: None,
                        valid_to: None,
                        open_end: true,
                    }),
                    scope: scope as i32,
                }),
                signature_summary: format!("{predicate_label}={preference}"),
                conflict_group_id: format!("conflict:{predicate_label}:{subject}:{}", scope.as_str_name()),
            };
            state.handles.insert(handle_ref.clone(), handle);
            handle_refs.push(handle_ref);
        }

        Ok(Response::new(AppendEventResponse {
            event_id,
            handle_refs,
        }))
    }

    async fn get_manifest(
        &self,
        request: Request<GetManifestRequest>,
    ) -> Result<Response<GetManifestResponse>, Status> {
        let req = request.into_inner();
        let state = self.state.lock().await;
        let mut handles = state.handles.values().cloned().collect::<Vec<_>>();
        handles.sort_by(|a, b| a.r#ref.cmp(&b.r#ref));

        let selectors = vec![SelectorRef {
            sel: "S0".to_string(),
            description: "Find preferences for subject".to_string(),
            params: vec![rmvm_proto::ParamSpec {
                name: "subject".to_string(),
                r#type: rmvm_proto::ParamType::ParamString as i32,
                enum_values: Vec::new(),
            }],
            cost_weight: 1.25,
            return_type: SelectorReturn::ReturnHandleSet as i32,
        }];

        Ok(Response::new(GetManifestResponse {
            manifest: Some(rmvm_proto::PublicManifest {
                request_id: req.request_id,
                handles,
                selectors,
                context: Vec::new(),
                budget: Some(rmvm_proto::PlanBudget {
                    max_ops: 128,
                    max_join_depth: 3,
                    max_fanout: 64,
                    max_total_cost: 256.0,
                }),
            }),
        }))
    }

    async fn execute(
        &self,
        request: Request<ExecuteRequest>,
    ) -> Result<Response<ExecuteResponse>, Status> {
        let response = execute(request.into_inner(), self.options.clone());
        Ok(Response::new(response))
    }

    async fn forget(
        &self,
        request: Request<ForgetRequest>,
    ) -> Result<Response<ForgetResponse>, Status> {
        let req = request.into_inner();
        let scope_filter = Scope::try_from(req.scope).unwrap_or(Scope::Unspecified);
        let mut state = self.state.lock().await;

        let mut remove_keys = Vec::new();
        for (key, handle) in &state.handles {
            let Some(meta) = handle.meta.as_ref() else {
                continue;
            };
            if !req.subject.is_empty() && meta.subject != req.subject {
                continue;
            }
            if !req.predicate_label.is_empty() && meta.predicate_label != req.predicate_label {
                continue;
            }
            if scope_filter != Scope::Unspecified && meta.scope != scope_filter as i32 {
                continue;
            }
            remove_keys.push(key.clone());
        }
        remove_keys.sort();
        for key in &remove_keys {
            state.handles.remove(key);
        }

        let citations = remove_keys
            .iter()
            .map(|h| CanonicalCitation {
                anchor_digest: format!("handle:{h}"),
            })
            .collect::<Vec<_>>();
        let assertions = vec![VerifiedAssertion {
            assertion_type: rmvm_proto::AssertionType::AssertDecision as i32,
            fields: BTreeMap::from([
                ("action".to_string(), value_string("suppressed_preference")),
                ("subject".to_string(), value_string(&req.subject)),
                ("predicate_label".to_string(), value_string(&req.predicate_label)),
                (
                    "suppressed_count".to_string(),
                    Value {
                        v: Some(V::I64(remove_keys.len() as i64)),
                    },
                ),
            ]),
            citations,
        }];

        let verified_blocks = vec![format!(
            "Suppressed {} preference handles for {} / {}.",
            remove_keys.len(),
            req.subject,
            req.predicate_label
        )];

        Ok(Response::new(ForgetResponse {
            status: ExecutionStatus::Ok as i32,
            assertions,
            rendered: Some(RenderedOutput {
                verified_blocks,
                narrative_blocks: Vec::new(),
            }),
            error: None,
        }))
    }
}

fn parse_preference(text: &str) -> Option<(&'static str, String)> {
    let normalized = text.trim();
    let lower = normalized.to_ascii_lowercase();

    for prefix in ["i prefer ", "prefer "] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let value = if normalized.len() >= prefix.len() {
                normalized[prefix.len()..].trim().to_string()
            } else {
                rest.trim().to_string()
            };
            if !value.is_empty() {
                return Some(("prefers_beverage", value));
            }
        }
    }
    None
}

fn value_string(v: &str) -> Value {
    Value {
        v: Some(V::S(v.to_string())),
    }
}
