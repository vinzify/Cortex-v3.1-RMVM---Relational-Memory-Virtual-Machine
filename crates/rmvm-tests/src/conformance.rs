use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use prost::Message;
use rmvm_kernel::{ExecuteOptions, execute};
use rmvm_proto::cortex::rmvm::v3_1::citation_ref::Cite;
use rmvm_proto::cortex::rmvm::v3_1::step::Op;
use rmvm_proto::cortex::rmvm::v3_1::value::V;
use rmvm_proto::{
    AssertionType, CitationRef, ContextVar, EdgeType, ErrorCode, ExecuteRequest, ExecuteResponse,
    ExecutionStatus, HandleAvailability, HandleMeta, HandleRef, OpApplySelector, OpAssert, OpFetch,
    OpFilter, OpJoin, OpProject, OpResolve, OutputSpec, ParamSpec, ParamType, PlanBudget,
    PublicManifest, RmvmPlan, Scope, SelectorRef, SelectorReturn, Step, TemporalBound, TaintClass,
    TrustTier, Value, ValueRef, assertion_type_from_i32, edge_type_from_i32, sha256_hex,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

pub const CONFORMANCE_SPEC_VERSION: &str = "conformance/v1.0.0";
pub const CONFORMANCE_PROTO_VERSION: &str = "cortex_rmvm_v3_1";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConformanceVector {
    pub vector_id: String,
    pub spec_version: String,
    pub proto_version: String,
    pub description: String,
    pub manifest: ManifestInput,
    pub plan: PlanInput,
    #[serde(default)]
    pub execute_options: ExecuteOptionsInput,
    pub expect: ExpectedOutcome,
    #[serde(default)]
    pub determinism: DeterminismExpectations,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ManifestInput {
    pub request_id: String,
    #[serde(default)]
    pub handles: Vec<HandleInput>,
    #[serde(default)]
    pub selectors: Vec<SelectorInput>,
    #[serde(default)]
    pub context: Vec<ContextVarInput>,
    pub budget: Option<BudgetInput>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HandleInput {
    pub r#ref: String,
    pub type_id: String,
    pub availability: String,
    pub subject: Option<String>,
    pub predicate_label: Option<String>,
    pub trust_tier: Option<String>,
    #[serde(default)]
    pub taint: Vec<String>,
    pub scope: Option<String>,
    pub signature_summary: Option<String>,
    pub conflict_group_id: Option<String>,
    pub open_end: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SelectorInput {
    pub sel: String,
    pub description: String,
    #[serde(default)]
    pub params: Vec<ParamInput>,
    pub cost_weight: f64,
    pub return_type: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ParamInput {
    pub name: String,
    pub r#type: String,
    #[serde(default)]
    pub enum_values: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContextVarInput {
    pub name: String,
    pub value: ScalarValueInput,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BudgetInput {
    pub max_ops: u32,
    pub max_join_depth: u32,
    pub max_fanout: u32,
    pub max_total_cost: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlanInput {
    pub request_id: String,
    pub steps: Vec<StepInput>,
    #[serde(default)]
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StepInput {
    pub out: String,
    pub op: OpInput,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OpInput {
    Fetch {
        handle_ref: String,
    },
    ApplySelector {
        selector_ref: String,
        #[serde(default)]
        params: BTreeMap<String, ScalarValueInput>,
    },
    Resolve {
        in_reg: String,
        #[serde(default)]
        policy_id: String,
    },
    Filter {
        in_reg: String,
        filter_ref: String,
        #[serde(default)]
        params: BTreeMap<String, ScalarValueInput>,
    },
    Join {
        left_reg: String,
        right_reg: String,
        edge_type: String,
    },
    Project {
        in_reg: String,
        field_paths: Vec<String>,
    },
    Assert {
        assertion_type: String,
        bindings: BTreeMap<String, BindingInput>,
        #[serde(default)]
        citations: Vec<CitationInput>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BindingInput {
    pub reg: String,
    pub field_path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum CitationInput {
    HandleRef(String),
    AnchorRef(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ScalarValueInput {
    S(String),
    B(bool),
    I64(i64),
    F64(f64),
    E(String),
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ExecuteOptionsInput {
    #[serde(default)]
    pub allow_partial_on_stall: bool,
    #[serde(default)]
    pub degraded_mode: bool,
    #[serde(default)]
    pub broken_lineage_handles: Vec<String>,
    #[serde(default)]
    pub narrative_templates: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExpectedOutcome {
    pub status: String,
    pub error_code: Option<String>,
    pub semantic_root: Option<String>,
    pub verified_blocks: Option<Vec<String>>,
    pub stall: Option<ExpectedStall>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExpectedStall {
    pub handle_ref: String,
    pub availability: String,
    pub retrieval_ticket_present: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DeterminismExpectations {
    pub assert_response_cpe_sha256: Option<String>,
    #[serde(default = "true_default")]
    pub assert_semantic_root: bool,
}

fn true_default() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct VectorReport {
    pub vector_id: String,
    pub status: String,
    pub success: bool,
    pub failures: Vec<String>,
    pub response_sha256: String,
    pub response_len: usize,
    pub semantic_root: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub response: ExecuteResponse,
    pub response_bytes: Vec<u8>,
    pub response_sha256: String,
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("failed to resolve repo root")
}

pub fn conformance_root() -> PathBuf {
    repo_root().join("tests").join("conformance").join("v1")
}

pub fn vector_schema_path() -> PathBuf {
    conformance_root().join("schema").join("vector.schema.json")
}

pub fn vector_paths() -> Vec<PathBuf> {
    let root = conformance_root().join("vectors");
    let mut files = WalkDir::new(&root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().and_then(|e| e.to_str()) == Some("json"))
        .map(|e| e.path().to_path_buf())
        .collect::<Vec<_>>();
    files.sort();
    files
}

pub fn load_vector(path: &Path) -> Result<ConformanceVector, String> {
    let raw = fs::read_to_string(path)
        .map_err(|e| format!("failed reading vector {}: {e}", path.display()))?;
    let value: JsonValue = serde_json::from_str(&raw)
        .map_err(|e| format!("invalid json {}: {e}", path.display()))?;
    let vec: ConformanceVector = serde_json::from_value(value)
        .map_err(|e| format!("invalid vector payload {}: {e}", path.display()))?;
    Ok(vec)
}

pub fn load_vectors() -> Result<Vec<(PathBuf, ConformanceVector)>, String> {
    let mut out = Vec::new();
    for path in vector_paths() {
        out.push((path.clone(), load_vector(&path)?));
    }
    Ok(out)
}

pub fn validate_vector_conventions(path: &Path, vector: &ConformanceVector) -> Vec<String> {
    let mut errors = Vec::new();
    if vector.spec_version != CONFORMANCE_SPEC_VERSION {
        errors.push(format!(
            "spec_version mismatch: expected {}, got {}",
            CONFORMANCE_SPEC_VERSION, vector.spec_version
        ));
    }
    if vector.proto_version != CONFORMANCE_PROTO_VERSION {
        errors.push(format!(
            "proto_version mismatch: expected {}, got {}",
            CONFORMANCE_PROTO_VERSION, vector.proto_version
        ));
    }
    if !vector
        .vector_id
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-' || c.is_ascii_lowercase())
    {
        errors.push("vector_id contains invalid characters".to_string());
    }
    let filename = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    if filename != vector.vector_id {
        errors.push(format!(
            "filename ({filename}) does not match vector_id ({})",
            vector.vector_id
        ));
    }
    errors
}

pub fn to_execute_request(vector: &ConformanceVector) -> Result<ExecuteRequest, String> {
    let manifest = PublicManifest {
        request_id: vector.manifest.request_id.clone(),
        handles: vector
            .manifest
            .handles
            .iter()
            .map(to_handle_ref)
            .collect::<Result<Vec<_>, _>>()?,
        selectors: vector
            .manifest
            .selectors
            .iter()
            .map(to_selector_ref)
            .collect::<Result<Vec<_>, _>>()?,
        context: vector
            .manifest
            .context
            .iter()
            .map(|c| ContextVar {
                name: c.name.clone(),
                value: Some(to_value(&c.value)),
            })
            .collect(),
        budget: vector.manifest.budget.as_ref().map(|b| PlanBudget {
            max_ops: b.max_ops,
            max_join_depth: b.max_join_depth,
            max_fanout: b.max_fanout,
            max_total_cost: b.max_total_cost,
        }),
    };

    let plan = RmvmPlan {
        request_id: vector.plan.request_id.clone(),
        steps: vector
            .plan
            .steps
            .iter()
            .map(to_step)
            .collect::<Result<Vec<_>, _>>()?,
        outputs: vector
            .plan
            .outputs
            .iter()
            .map(|o| OutputSpec { reg: o.clone() })
            .collect(),
    };

    Ok(ExecuteRequest {
        manifest: Some(manifest),
        plan: Some(plan),
    })
}

pub fn to_execute_options(input: &ExecuteOptionsInput) -> ExecuteOptions {
    ExecuteOptions {
        allow_partial_on_stall: input.allow_partial_on_stall,
        degraded_mode: input.degraded_mode,
        broken_lineage_handles: input.broken_lineage_handles.iter().cloned().collect(),
        narrative_templates: input.narrative_templates.clone(),
    }
}

pub fn run_vector(vector: &ConformanceVector) -> Result<ExecutionResult, String> {
    let request = to_execute_request(vector)?;
    let response = execute(request, to_execute_options(&vector.execute_options));
    let response_bytes = response.encode_to_vec();
    let mut hasher = Sha256::new();
    hasher.update(&response_bytes);
    let response_sha256 = hex::encode(hasher.finalize());
    Ok(ExecutionResult {
        response,
        response_bytes,
        response_sha256,
    })
}

pub fn compare_expected(vector: &ConformanceVector, result: &ExecutionResult) -> Vec<String> {
    let mut failures = Vec::new();
    let actual_status = status_name(result.response.status);
    if actual_status != vector.expect.status {
        failures.push(format!(
            "status mismatch: expected {}, got {}",
            vector.expect.status, actual_status
        ));
    }

    let expected_error = vector.expect.error_code.clone().unwrap_or_default();
    let actual_error = result
        .response
        .error
        .as_ref()
        .map(|e| error_code_name(e.code))
        .unwrap_or_default();
    if !expected_error.is_empty() && expected_error != actual_error {
        failures.push(format!(
            "error_code mismatch: expected {}, got {}",
            expected_error, actual_error
        ));
    }

    if let Some(ref expected_root) = vector.expect.semantic_root {
        let actual = result
            .response
            .proof
            .as_ref()
            .map(|p| p.semantic_root.clone())
            .unwrap_or_default();
        if expected_root != &actual {
            failures.push(format!(
                "semantic_root mismatch: expected {}, got {}",
                expected_root, actual
            ));
        }
    } else if vector.determinism.assert_semantic_root
        && actual_status == ExecutionStatus::Ok.as_str_name()
        && result
            .response
            .proof
            .as_ref()
            .map(|p| p.semantic_root.is_empty())
            .unwrap_or(true)
    {
        failures.push("semantic_root missing for OK response".to_string());
    }

    if let Some(ref expected_blocks) = vector.expect.verified_blocks {
        let actual_blocks = result
            .response
            .rendered
            .as_ref()
            .map(|r| r.verified_blocks.clone())
            .unwrap_or_default();
        if expected_blocks != &actual_blocks {
            failures.push(format!(
                "verified_blocks mismatch: expected {:?}, got {:?}",
                expected_blocks, actual_blocks
            ));
        }
    }

    if let Some(ref stall_expect) = vector.expect.stall {
        let stall = result.response.stall.as_ref();
        if stall.is_none() {
            failures.push("expected STALL info but response has none".to_string());
        } else if let Some(s) = stall {
            if s.handle_ref != stall_expect.handle_ref {
                failures.push(format!(
                    "stall.handle_ref mismatch: expected {}, got {}",
                    stall_expect.handle_ref, s.handle_ref
                ));
            }
            if availability_name(s.availability) != stall_expect.availability {
                failures.push(format!(
                    "stall.availability mismatch: expected {}, got {}",
                    stall_expect.availability,
                    availability_name(s.availability)
                ));
            }
            let has_ticket = !s.retrieval_ticket.trim().is_empty();
            if has_ticket != stall_expect.retrieval_ticket_present {
                failures.push(format!(
                    "stall.retrieval_ticket_present mismatch: expected {}, got {}",
                    stall_expect.retrieval_ticket_present, has_ticket
                ));
            }
        }
    }

    if let Some(ref expected_hash) = vector.determinism.assert_response_cpe_sha256 {
        if expected_hash != &result.response_sha256 {
            failures.push(format!(
                "response sha256 mismatch: expected {}, got {}",
                expected_hash, result.response_sha256
            ));
        }
    }

    failures
}

pub fn vector_report(vector: &ConformanceVector, result: &ExecutionResult, failures: Vec<String>) -> VectorReport {
    VectorReport {
        vector_id: vector.vector_id.clone(),
        status: status_name(result.response.status).to_string(),
        success: failures.is_empty(),
        failures,
        response_sha256: result.response_sha256.clone(),
        response_len: result.response_bytes.len(),
        semantic_root: result
            .response
            .proof
            .as_ref()
            .map(|p| p.semantic_root.clone()),
    }
}

pub fn write_report(root: &Path, run_id: &str, os_id: &str, report: &VectorReport) -> Result<(), String> {
    let dir = root.join("reports").join(run_id).join(os_id);
    fs::create_dir_all(&dir).map_err(|e| format!("failed creating report dir {}: {e}", dir.display()))?;
    let path = dir.join(format!("{}.json", report.vector_id));
    let payload = serde_json::to_string_pretty(report).map_err(|e| format!("failed serializing report: {e}"))?;
    fs::write(&path, payload).map_err(|e| format!("failed writing report {}: {e}", path.display()))
}

pub fn baseline_dir(vector_id: &str) -> PathBuf {
    conformance_root().join("baselines").join(vector_id)
}

pub fn write_baseline(vector: &ConformanceVector, result: &ExecutionResult) -> Result<(), String> {
    let dir = baseline_dir(&vector.vector_id);
    fs::create_dir_all(&dir).map_err(|e| format!("failed creating baseline dir {}: {e}", dir.display()))?;
    let pb_path = dir.join("expected.execute_response.cpe.pb");
    fs::write(&pb_path, &result.response_bytes)
        .map_err(|e| format!("failed writing baseline response {}: {e}", pb_path.display()))?;

    let meta = serde_json::json!({
        "vector_id": vector.vector_id,
        "status": status_name(result.response.status),
        "error_code": result.response.error.as_ref().map(|e| error_code_name(e.code)),
        "semantic_root": result.response.proof.as_ref().map(|p| p.semantic_root.clone()),
        "response_sha256": result.response_sha256,
    });
    let meta_path = dir.join("expected.meta.json");
    fs::write(
        &meta_path,
        serde_json::to_string_pretty(&meta).expect("meta json serialize"),
    )
    .map_err(|e| format!("failed writing baseline meta {}: {e}", meta_path.display()))?;
    Ok(())
}

pub fn load_baseline_hash(vector_id: &str) -> Option<String> {
    let bytes = load_baseline_bytes(vector_id)?;
    Some(sha256_hex(&bytes))
}

pub fn load_baseline_bytes(vector_id: &str) -> Option<Vec<u8>> {
    let path = baseline_dir(vector_id).join("expected.execute_response.cpe.pb");
    if !path.exists() {
        return None;
    }
    fs::read(path).ok()
}

pub fn write_drift_artifacts(
    root: &Path,
    run_id: &str,
    os_id: &str,
    vector_id: &str,
    expected_bytes: Option<&[u8]>,
    actual_bytes: &[u8],
    reason: &str,
) -> Result<(), String> {
    let dir = root
        .join("reports")
        .join(run_id)
        .join(os_id)
        .join("drift")
        .join(vector_id);
    fs::create_dir_all(&dir)
        .map_err(|e| format!("failed creating drift dir {}: {e}", dir.display()))?;

    let actual_path = dir.join("actual.execute_response.cpe.pb");
    fs::write(&actual_path, actual_bytes)
        .map_err(|e| format!("failed writing actual response {}: {e}", actual_path.display()))?;

    if let Some(expected) = expected_bytes {
        let expected_path = dir.join("expected.execute_response.cpe.pb");
        fs::write(&expected_path, expected).map_err(|e| {
            format!(
                "failed writing expected response {}: {e}",
                expected_path.display()
            )
        })?;
    }

    let meta = serde_json::json!({
      "vector_id": vector_id,
      "reason": reason,
      "actual_sha256": sha256_hex(actual_bytes),
      "expected_sha256": expected_bytes.map(sha256_hex),
      "actual_len": actual_bytes.len(),
      "expected_len": expected_bytes.map(|b| b.len()),
    });
    let diff_path = dir.join("diff.meta.json");
    fs::write(
        &diff_path,
        serde_json::to_string_pretty(&meta).expect("failed serializing diff meta"),
    )
    .map_err(|e| format!("failed writing diff meta {}: {e}", diff_path.display()))?;
    Ok(())
}

pub fn status_name(status: i32) -> &'static str {
    ExecutionStatus::try_from(status)
        .unwrap_or(ExecutionStatus::Unspecified)
        .as_str_name()
}

pub fn error_code_name(code: i32) -> &'static str {
    ErrorCode::try_from(code)
        .unwrap_or(ErrorCode::Unspecified)
        .as_str_name()
}

pub fn availability_name(v: i32) -> &'static str {
    HandleAvailability::try_from(v)
        .unwrap_or(HandleAvailability::Unspecified)
        .as_str_name()
}

fn to_handle_ref(input: &HandleInput) -> Result<HandleRef, String> {
    let availability = parse_availability(&input.availability)? as i32;
    let trust_tier = input
        .trust_tier
        .as_ref()
        .map(|t| parse_trust_tier(t).map(|v| v as i32))
        .transpose()?
        .unwrap_or(TrustTier::Tier3Confirmed as i32);
    let scope = input
        .scope
        .as_ref()
        .map(|s| parse_scope(s).map(|v| v as i32))
        .transpose()?
        .unwrap_or(Scope::Global as i32);
    let taint = input
        .taint
        .iter()
        .map(|t| parse_taint(t).map(|v| v as i32))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(HandleRef {
        r#ref: input.r#ref.clone(),
        type_id: input.type_id.clone(),
        availability,
        meta: Some(HandleMeta {
            subject: input.subject.clone().unwrap_or_default(),
            predicate_label: input.predicate_label.clone().unwrap_or_default(),
            trust_tier,
            taint,
            temporal: Some(TemporalBound {
                valid_from: None,
                valid_to: None,
                open_end: input.open_end.unwrap_or(true),
            }),
            scope,
        }),
        signature_summary: input.signature_summary.clone().unwrap_or_default(),
        conflict_group_id: input.conflict_group_id.clone().unwrap_or_default(),
    })
}

fn to_selector_ref(input: &SelectorInput) -> Result<SelectorRef, String> {
    Ok(SelectorRef {
        sel: input.sel.clone(),
        description: input.description.clone(),
        params: input
            .params
            .iter()
            .map(|p| {
                Ok(ParamSpec {
                    name: p.name.clone(),
                    r#type: parse_param_type(&p.r#type)? as i32,
                    enum_values: p.enum_values.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        cost_weight: input.cost_weight,
        return_type: parse_selector_return(&input.return_type)? as i32,
    })
}

fn to_step(input: &StepInput) -> Result<Step, String> {
    let op = match &input.op {
        OpInput::Fetch { handle_ref } => Op::Fetch(OpFetch {
            handle_ref: handle_ref.clone(),
        }),
        OpInput::ApplySelector {
            selector_ref,
            params,
        } => Op::ApplySelector(OpApplySelector {
            selector_ref: selector_ref.clone(),
            params: params
                .iter()
                .map(|(k, v)| (k.clone(), to_value(v)))
                .collect::<BTreeMap<_, _>>(),
        }),
        OpInput::Resolve { in_reg, policy_id } => Op::Resolve(OpResolve {
            in_reg: in_reg.clone(),
            policy_id: policy_id.clone(),
        }),
        OpInput::Filter {
            in_reg,
            filter_ref,
            params,
        } => Op::Filter(OpFilter {
            in_reg: in_reg.clone(),
            filter_ref: filter_ref.clone(),
            params: params
                .iter()
                .map(|(k, v)| (k.clone(), to_value(v)))
                .collect::<BTreeMap<_, _>>(),
        }),
        OpInput::Join {
            left_reg,
            right_reg,
            edge_type,
        } => Op::Join(OpJoin {
            left_reg: left_reg.clone(),
            right_reg: right_reg.clone(),
            edge_type: parse_edge_type(edge_type)? as i32,
        }),
        OpInput::Project { in_reg, field_paths } => Op::Project(OpProject {
            in_reg: in_reg.clone(),
            field_paths: field_paths.clone(),
        }),
        OpInput::Assert {
            assertion_type,
            bindings,
            citations,
        } => Op::AssertOp(OpAssert {
            assertion_type: parse_assertion_type(assertion_type)? as i32,
            bindings: bindings
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        ValueRef {
                            reg: v.reg.clone(),
                            field_path: v.field_path.clone(),
                        },
                    )
                })
                .collect(),
            citations: citations
                .iter()
                .map(|c| match c {
                    CitationInput::HandleRef(v) => CitationRef {
                        cite: Some(Cite::HandleRef(v.clone())),
                    },
                    CitationInput::AnchorRef(v) => CitationRef {
                        cite: Some(Cite::AnchorRef(v.clone())),
                    },
                })
                .collect(),
        }),
    };
    Ok(Step {
        out: input.out.clone(),
        op: Some(op),
    })
}

fn to_value(input: &ScalarValueInput) -> Value {
    let v = match input {
        ScalarValueInput::S(v) => V::S(v.clone()),
        ScalarValueInput::B(v) => V::B(*v),
        ScalarValueInput::I64(v) => V::I64(*v),
        ScalarValueInput::F64(v) => V::F64(*v),
        ScalarValueInput::E(v) => V::E(v.clone()),
    };
    Value { v: Some(v) }
}

fn parse_availability(v: &str) -> Result<HandleAvailability, String> {
    HandleAvailability::from_str_name(v).ok_or_else(|| format!("invalid HandleAvailability: {v}"))
}

fn parse_trust_tier(v: &str) -> Result<TrustTier, String> {
    TrustTier::from_str_name(v).ok_or_else(|| format!("invalid TrustTier: {v}"))
}

fn parse_scope(v: &str) -> Result<Scope, String> {
    Scope::from_str_name(v).ok_or_else(|| format!("invalid Scope: {v}"))
}

fn parse_taint(v: &str) -> Result<TaintClass, String> {
    TaintClass::from_str_name(v).ok_or_else(|| format!("invalid TaintClass: {v}"))
}

fn parse_param_type(v: &str) -> Result<ParamType, String> {
    ParamType::from_str_name(v).ok_or_else(|| format!("invalid ParamType: {v}"))
}

fn parse_selector_return(v: &str) -> Result<SelectorReturn, String> {
    SelectorReturn::from_str_name(v).ok_or_else(|| format!("invalid SelectorReturn: {v}"))
}

fn parse_edge_type(v: &str) -> Result<EdgeType, String> {
    EdgeType::from_str_name(v).ok_or_else(|| format!("invalid EdgeType: {v}"))
}

fn parse_assertion_type(v: &str) -> Result<AssertionType, String> {
    AssertionType::from_str_name(v).ok_or_else(|| format!("invalid AssertionType: {v}"))
}

#[allow(dead_code)]
fn _assertion_name(v: i32) -> &'static str {
    assertion_type_from_i32(v).as_str_name()
}

#[allow(dead_code)]
fn _edge_name(v: i32) -> &'static str {
    edge_type_from_i32(v).as_str_name()
}

pub fn update_vector_expectations(path: &Path, vector: &ConformanceVector, result: &ExecutionResult) -> Result<(), String> {
    let mut updated = vector.clone();
    updated.expect.status = status_name(result.response.status).to_string();
    updated.expect.error_code = result
        .response
        .error
        .as_ref()
        .map(|e| error_code_name(e.code).to_string());
    updated.expect.semantic_root = result
        .response
        .proof
        .as_ref()
        .map(|p| p.semantic_root.clone());
    updated.expect.verified_blocks = result
        .response
        .rendered
        .as_ref()
        .map(|r| r.verified_blocks.clone());
    updated.expect.stall = result.response.stall.as_ref().map(|s| ExpectedStall {
        handle_ref: s.handle_ref.clone(),
        availability: availability_name(s.availability).to_string(),
        retrieval_ticket_present: !s.retrieval_ticket.trim().is_empty(),
    });
    updated.determinism.assert_response_cpe_sha256 = Some(result.response_sha256.clone());

    let serialized =
        serde_json::to_string_pretty(&updated).map_err(|e| format!("failed to serialize updated vector: {e}"))?;
    fs::write(path, format!("{serialized}\n"))
        .map_err(|e| format!("failed to write vector {}: {e}", path.display()))
}
