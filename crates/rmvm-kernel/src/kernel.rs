use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;
use rmvm_proto::cortex::rmvm::v3_1::citation_ref::Cite;
use rmvm_proto::cortex::rmvm::v3_1::step::Op;
use rmvm_proto::cortex::rmvm::v3_1::value::V;
use rmvm_proto::{
    AssertionMerkleProof, AssertionType, CanonicalCitation, CitationRef, EdgeType, ErrorCode,
    ExecuteRequest, ExecuteResponse, ExecutionError, ExecutionStatus, HandleAvailability, HandleRef,
    Hint, InclusionProof, OpApplySelector, OpAssert, OpFetch, OpFilter, OpJoin, OpProject,
    OpResolve, OutputSpec, ParamSpec, ParamType, PlanBudget, PublicManifest, RmvmPlan,
    RenderedOutput, SelectorRef, SelectorReturn, StallInfo, TaintClass, TrustTier, Value, ValueRef,
    assertion_type_from_i32, availability_from_i32, canonical_assertion_hash, edge_type_from_i32,
    param_type_from_i32, selector_return_from_i32, sha256_hex, trust_tier_from_i32,
};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct ExecuteOptions {
    pub allow_partial_on_stall: bool,
    pub degraded_mode: bool,
    pub broken_lineage_handles: BTreeSet<String>,
    pub narrative_templates: Vec<String>,
}

impl Default for ExecuteOptions {
    fn default() -> Self {
        Self {
            allow_partial_on_stall: false,
            degraded_mode: false,
            broken_lineage_handles: BTreeSet::new(),
            narrative_templates: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct NormalizedBudget {
    max_ops: u32,
    max_join_depth: u32,
    max_fanout: u32,
    max_total_cost: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    Fetch,
    ApplySelector,
    Resolve,
    Filter,
    Join,
    Project,
    Assert,
}

#[derive(Debug, Clone)]
enum RegData {
    Handle(HandleRef),
    HandleSet(Vec<HandleRef>),
    Struct(BTreeMap<String, Value>),
}

#[derive(Debug, Clone)]
struct RegValue {
    data: RegData,
    provenance: BTreeSet<String>,
    taint: BTreeSet<i32>,
    min_trust: TrustTier,
    join_depth: u32,
    source: SourceKind,
}

#[derive(Debug, Clone)]
struct KernelFailure {
    status: ExecutionStatus,
    code: ErrorCode,
    message: String,
    hints: Vec<Hint>,
}

#[derive(Debug, Clone)]
enum ExecStop {
    Failure(KernelFailure),
    Stall(StallInfo),
}

pub fn execute(req: ExecuteRequest, opts: ExecuteOptions) -> ExecuteResponse {
    match execute_inner(req, &opts) {
        Ok((assertions, proof, rendered)) => ExecuteResponse {
            status: ExecutionStatus::Ok as i32,
            assertions,
            proof: Some(proof),
            rendered: Some(rendered),
            stall: None,
            error: None,
        },
        Err(ExecStop::Stall(stall)) => ExecuteResponse {
            status: ExecutionStatus::Stall as i32,
            assertions: Vec::new(),
            proof: None,
            rendered: None,
            stall: Some(stall),
            error: None,
        },
        Err(ExecStop::Failure(f)) => ExecuteResponse {
            status: f.status as i32,
            assertions: Vec::new(),
            proof: None,
            rendered: None,
            stall: None,
            error: Some(ExecutionError {
                code: f.code as i32,
                message: f.message,
                hints: f.hints,
            }),
        },
    }
}

fn execute_inner(
    req: ExecuteRequest,
    opts: &ExecuteOptions,
) -> Result<(Vec<rmvm_proto::VerifiedAssertion>, AssertionMerkleProof, RenderedOutput), ExecStop> {
    let manifest = req
        .manifest
        .ok_or_else(|| fail_rejected(ErrorCode::SchemaViolation, "missing manifest"))?;
    let plan = req
        .plan
        .ok_or_else(|| fail_rejected(ErrorCode::SchemaViolation, "missing plan"))?;

    if !manifest.request_id.is_empty()
        && !plan.request_id.is_empty()
        && manifest.request_id != plan.request_id
    {
        return Err(fail_rejected(
            ErrorCode::SchemaViolation,
            "manifest and plan request_id mismatch",
        ));
    }

    let budget = normalize_budget(manifest.budget.as_ref());
    let handles_by_ref = handles_index(&manifest.handles);
    let selectors_by_ref = selectors_index(&manifest.selectors);
    validate_static(&manifest, &plan, &budget, &selectors_by_ref)?;

    let mut regs: BTreeMap<String, RegValue> = BTreeMap::new();
    let mut assertions: Vec<rmvm_proto::VerifiedAssertion> = Vec::new();
    let mut trace_tokens: Vec<String> = Vec::new();

    for (idx, step) in plan.steps.iter().enumerate() {
        let out = step.out.clone();
        let op = step
            .op
            .as_ref()
            .ok_or_else(|| fail_rejected(ErrorCode::SchemaViolation, "step missing op"))?;

        let (result, opname) = match op {
            Op::Fetch(fetch) => (exec_fetch(fetch, &handles_by_ref, opts)?, "FETCH"),
            Op::ApplySelector(apply) => (
                exec_apply_selector(apply, &manifest, &selectors_by_ref, &budget)?,
                "APPLY_SELECTOR",
            ),
            Op::Resolve(resolve) => (exec_resolve(resolve, &regs)?, "RESOLVE"),
            Op::Filter(filter) => (exec_filter(filter, &regs, &budget)?, "FILTER"),
            Op::Join(join) => (exec_join(join, &regs, &budget)?, "JOIN"),
            Op::Project(project) => (exec_project(project, &regs)?, "PROJECT"),
            Op::AssertOp(assert_op) => {
                let (reg, assertion) = exec_assert(assert_op, &regs, opts)?;
                assertions.push(assertion);
                (reg, "ASSERT")
            }
        };

        trace_tokens.push(format!("{idx}:{out}:{opname}"));
        regs.insert(out, result);
    }

    let proof = build_proof(&assertions, &trace_tokens);
    let rendered = render_output(&plan.outputs, &regs, &assertions, opts)?;
    Ok((assertions, proof, rendered))
}

fn validate_static(
    manifest: &PublicManifest,
    plan: &RmvmPlan,
    budget: &NormalizedBudget,
    selectors_by_ref: &BTreeMap<String, SelectorRef>,
) -> Result<(), ExecStop> {
    if plan.steps.len() as u32 > budget.max_ops {
        return Err(fail_rejected(
            ErrorCode::GraphTraversalLimit,
            "max_ops exceeded",
        ));
    }

    let mut declared = BTreeSet::new();
    let mut join_depth: BTreeMap<String, u32> = BTreeMap::new();
    let mut total_cost = 0.0f64;

    for step in &plan.steps {
        if step.out.trim().is_empty() {
            return Err(fail_rejected(
                ErrorCode::SchemaViolation,
                "step.out cannot be empty",
            ));
        }
        if !declared.insert(step.out.clone()) {
            return Err(fail_rejected(
                ErrorCode::SchemaViolation,
                "register redefinition",
            ));
        }

        let op = step
            .op
            .as_ref()
            .ok_or_else(|| fail_rejected(ErrorCode::SchemaViolation, "step missing op"))?;

        let mut out_depth = 0u32;
        match op {
            Op::Fetch(OpFetch { handle_ref }) => {
                if !manifest
                    .handles
                    .iter()
                    .any(|h| h.r#ref == handle_ref.as_str())
                {
                    return Err(fail_rejected(
                        ErrorCode::UnknownHandleRef,
                        "unknown handle_ref in FETCH",
                    ));
                }
                total_cost += 1.0;
            }
            Op::ApplySelector(OpApplySelector {
                selector_ref,
                params,
            }) => {
                let selector = selectors_by_ref.get(selector_ref.as_str()).ok_or_else(|| {
                    fail_rejected(ErrorCode::UnknownSelectorRef, "unknown selector_ref")
                })?;
                validate_param_specs(&selector.params, &params)?;
                let rows = estimate_selector_rows(selector, manifest, budget).max(1) as f64;
                let weight = if selector.cost_weight <= 0.0 {
                    1.0
                } else {
                    selector.cost_weight
                };
                total_cost += weight * rows.powf(1.2);
            }
            Op::Resolve(OpResolve { in_reg, .. })
            | Op::Filter(OpFilter { in_reg, .. })
            | Op::Project(OpProject { in_reg, .. }) => {
                if !declared.contains(in_reg.as_str()) {
                    return Err(fail_rejected(
                        ErrorCode::SchemaViolation,
                        "register used before definition",
                    ));
                }
                out_depth = *join_depth.get(in_reg.as_str()).unwrap_or(&0);
                total_cost += 1.0;
            }
            Op::Join(OpJoin {
                left_reg,
                right_reg,
                ..
            }) => {
                if !declared.contains(left_reg.as_str()) || !declared.contains(right_reg.as_str()) {
                    return Err(fail_rejected(
                        ErrorCode::SchemaViolation,
                        "JOIN register used before definition",
                    ));
                }
                out_depth = join_depth.get(left_reg.as_str()).copied().unwrap_or(0).max(
                    join_depth.get(right_reg.as_str()).copied().unwrap_or(0),
                ) + 1;
                if out_depth > budget.max_join_depth {
                    return Err(fail_rejected(
                        ErrorCode::GraphTraversalLimit,
                        "max_join_depth exceeded",
                    ));
                }
                total_cost += 2.0;
            }
            Op::AssertOp(OpAssert { bindings, .. }) => {
                for ValueRef { reg, .. } in bindings.values() {
                    if !declared.contains(reg.as_str()) {
                        return Err(fail_rejected(
                            ErrorCode::SchemaViolation,
                            "ASSERT binding uses undefined register",
                        ));
                    }
                }
                total_cost += 1.0;
            }
        }

        if out_depth > 0 {
            join_depth.insert(step.out.clone(), out_depth);
        }
    }

    if total_cost > budget.max_total_cost {
        return Err(fail_rejected(
            ErrorCode::CostGuardRejected,
            "plan rejected by COST_GUARD",
        ));
    }

    for OutputSpec { reg } in &plan.outputs {
        if !declared.contains(reg) {
            return Err(fail_rejected(
                ErrorCode::SchemaViolation,
                "undefined output register",
            ));
        }
    }
    Ok(())
}

fn exec_fetch(
    fetch: &OpFetch,
    handles_by_ref: &BTreeMap<String, HandleRef>,
    opts: &ExecuteOptions,
) -> Result<RegValue, ExecStop> {
    let handle = handles_by_ref.get(&fetch.handle_ref).ok_or_else(|| {
        fail_rejected(ErrorCode::UnknownHandleRef, "unknown handle_ref in FETCH execution")
    })?;
    let availability = availability_from_i32(handle.availability);
    if availability != HandleAvailability::Ready {
        let stall = StallInfo {
            handle_ref: handle.r#ref.clone(),
            availability: handle.availability,
            estimated_ready_at: None,
            retrieval_ticket: format!("ticket:{}", handle.r#ref),
        };
        if !opts.allow_partial_on_stall {
            return Err(ExecStop::Stall(stall));
        }
        return Ok(RegValue {
            data: RegData::HandleSet(Vec::new()),
            provenance: BTreeSet::new(),
            taint: BTreeSet::new(),
            min_trust: TrustTier::Unspecified,
            join_depth: 0,
            source: SourceKind::Fetch,
        });
    }

    let mut provenance = BTreeSet::new();
    provenance.insert(handle.r#ref.clone());
    let taint = handle
        .meta
        .as_ref()
        .map(|m| m.taint.iter().copied().collect::<BTreeSet<_>>())
        .unwrap_or_default();
    Ok(RegValue {
        data: RegData::Handle(handle.clone()),
        provenance,
        taint,
        min_trust: handle
            .meta
            .as_ref()
            .map(|m| trust_tier_from_i32(m.trust_tier))
            .unwrap_or(TrustTier::Unspecified),
        join_depth: 0,
        source: SourceKind::Fetch,
    })
}

fn exec_apply_selector(
    apply: &OpApplySelector,
    manifest: &PublicManifest,
    selectors_by_ref: &BTreeMap<String, SelectorRef>,
    budget: &NormalizedBudget,
) -> Result<RegValue, ExecStop> {
    let selector = selectors_by_ref.get(&apply.selector_ref).ok_or_else(|| {
        fail_rejected(
            ErrorCode::UnknownSelectorRef,
            "unknown selector_ref in APPLY_SELECTOR execution",
        )
    })?;
    validate_param_specs(&selector.params, &apply.params)?;

    let mut matched: Vec<HandleRef> = manifest
        .handles
        .iter()
        .filter(|h| selector_match(h, &apply.params))
        .cloned()
        .collect();
    matched.sort_by(|a, b| a.r#ref.cmp(&b.r#ref));
    if matched.len() as u32 > budget.max_fanout {
        return Err(fail_rejected(
            ErrorCode::GraphTraversalLimit,
            "selector fanout exceeded max_fanout",
        ));
    }

    let mut provenance = BTreeSet::new();
    let mut taint = BTreeSet::new();
    let mut min_trust = TrustTier::Tier4PolicySigned;
    for handle in &matched {
        provenance.insert(handle.r#ref.clone());
        if let Some(meta) = handle.meta.as_ref() {
            taint.extend(meta.taint.iter().copied());
        }
        min_trust = min_tier(
            min_trust,
            handle
                .meta
                .as_ref()
                .map(|m| trust_tier_from_i32(m.trust_tier))
                .unwrap_or(TrustTier::Unspecified),
        );
    }
    if matched.is_empty() {
        min_trust = TrustTier::Unspecified;
    }

    let data = match selector_return_from_i32(selector.return_type) {
        SelectorReturn::ReturnHandle => {
            let first = matched
                .first()
                .cloned()
                .ok_or_else(|| fail_rejected(ErrorCode::TypeMismatch, "selector returned zero rows"))?;
            RegData::Handle(first)
        }
        SelectorReturn::ReturnHandleSet => RegData::HandleSet(matched),
        SelectorReturn::ReturnStruct => {
            let mut out = BTreeMap::new();
            out.insert(
                "matched_count".to_string(),
                Value {
                    v: Some(V::I64(manifest.handles.len() as i64)),
                },
            );
            for (k, v) in &apply.params {
                out.insert(k.clone(), v.clone());
            }
            RegData::Struct(out)
        }
        SelectorReturn::Unspecified => {
            return Err(fail_rejected(
                ErrorCode::SchemaViolation,
                "selector return_type unspecified",
            ));
        }
    };

    Ok(RegValue {
        data,
        provenance,
        taint,
        min_trust,
        join_depth: 0,
        source: SourceKind::ApplySelector,
    })
}

fn exec_resolve(resolve: &OpResolve, regs: &BTreeMap<String, RegValue>) -> Result<RegValue, ExecStop> {
    let input = regs.get(&resolve.in_reg).ok_or_else(|| {
        fail_rejected(ErrorCode::SchemaViolation, "RESOLVE input register missing")
    })?;
    let handles = as_handles(input)?;
    if handles.is_empty() {
        return Err(fail_rejected(
            ErrorCode::AmbiguousConflict,
            "RESOLVE candidates empty",
        ));
    }

    let mut ranked: Vec<(i32, i64, String, HandleRef)> = handles
        .into_iter()
        .map(|h| {
            let trust = h
                .meta
                .as_ref()
                .map(|m| trust_rank(trust_tier_from_i32(m.trust_tier)))
                .unwrap_or(0);
            let ts = h
                .meta
                .as_ref()
                .and_then(|m| m.temporal.as_ref())
                .and_then(|t| t.valid_from.as_ref())
                .map(timestamp_score)
                .unwrap_or(0);
            (trust, ts, h.r#ref.clone(), h)
        })
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)).then_with(|| a.2.cmp(&b.2)));

    if ranked.len() > 1 && ranked[0].0 == ranked[1].0 && ranked[0].1 == ranked[1].1 {
        return Err(fail_rejected(
            ErrorCode::AmbiguousConflict,
            "RESOLVE tie cannot be broken",
        ));
    }
    let winner = ranked
        .into_iter()
        .next()
        .map(|(_, _, _, h)| h)
        .ok_or_else(|| fail_rejected(ErrorCode::AmbiguousConflict, "RESOLVE winner missing"))?;
    Ok(RegValue {
        data: RegData::Handle(winner),
        provenance: input.provenance.clone(),
        taint: input.taint.clone(),
        min_trust: input.min_trust,
        join_depth: input.join_depth,
        source: SourceKind::Resolve,
    })
}

fn exec_filter(
    filter: &OpFilter,
    regs: &BTreeMap<String, RegValue>,
    budget: &NormalizedBudget,
) -> Result<RegValue, ExecStop> {
    let input = regs
        .get(&filter.in_reg)
        .ok_or_else(|| fail_rejected(ErrorCode::SchemaViolation, "FILTER input register missing"))?;
    let handles = as_handles(input)?;
    let mut out = Vec::new();
    for handle in handles {
        if matches_filter(&handle, &filter.filter_ref, &filter.params)? {
            out.push(handle);
        }
    }
    out.sort_by(|a, b| a.r#ref.cmp(&b.r#ref));
    if out.len() as u32 > budget.max_fanout {
        return Err(fail_rejected(
            ErrorCode::GraphTraversalLimit,
            "filter fanout exceeded max_fanout",
        ));
    }
    Ok(RegValue {
        data: RegData::HandleSet(out),
        provenance: input.provenance.clone(),
        taint: input.taint.clone(),
        min_trust: input.min_trust,
        join_depth: input.join_depth,
        source: SourceKind::Filter,
    })
}

fn exec_join(
    join: &OpJoin,
    regs: &BTreeMap<String, RegValue>,
    budget: &NormalizedBudget,
) -> Result<RegValue, ExecStop> {
    let left = regs
        .get(&join.left_reg)
        .ok_or_else(|| fail_rejected(ErrorCode::SchemaViolation, "JOIN left register missing"))?;
    let right = regs
        .get(&join.right_reg)
        .ok_or_else(|| fail_rejected(ErrorCode::SchemaViolation, "JOIN right register missing"))?;
    let edge = edge_type_from_i32(join.edge_type);
    if edge == EdgeType::Unspecified {
        return Err(fail_rejected(
            ErrorCode::SchemaViolation,
            "JOIN edge_type unspecified",
        ));
    }

    let left_set = as_handles(left)?;
    let right_set = as_handles(right)?;
    let mut joined: BTreeMap<String, HandleRef> = BTreeMap::new();
    for l in &left_set {
        for r in &right_set {
            let include = match edge {
                EdgeType::EdgeConflictsWith => {
                    !l.conflict_group_id.is_empty()
                        && l.conflict_group_id == r.conflict_group_id
                        && l.r#ref != r.r#ref
                }
                EdgeType::EdgeSupersedes => l.type_id == r.type_id && l.r#ref != r.r#ref,
                EdgeType::EdgeProvenance => l.signature_summary == r.signature_summary,
                EdgeType::EdgeSameEntity => {
                    l.meta.as_ref().map(|m| &m.subject) == r.meta.as_ref().map(|m| &m.subject)
                }
                EdgeType::Unspecified => false,
            };
            if include {
                joined.insert(l.r#ref.clone(), l.clone());
                joined.insert(r.r#ref.clone(), r.clone());
            }
        }
    }
    if joined.len() as u32 > budget.max_fanout {
        return Err(fail_rejected(
            ErrorCode::GraphTraversalLimit,
            "join fanout exceeded max_fanout",
        ));
    }
    Ok(RegValue {
        data: RegData::HandleSet(joined.into_values().collect()),
        provenance: left.provenance.union(&right.provenance).cloned().collect(),
        taint: left.taint.union(&right.taint).copied().collect(),
        min_trust: min_tier(left.min_trust, right.min_trust),
        join_depth: left.join_depth.max(right.join_depth) + 1,
        source: SourceKind::Join,
    })
}

fn exec_project(
    project: &OpProject,
    regs: &BTreeMap<String, RegValue>,
) -> Result<RegValue, ExecStop> {
    let input = regs
        .get(&project.in_reg)
        .ok_or_else(|| fail_rejected(ErrorCode::SchemaViolation, "PROJECT input register missing"))?;

    let mut out = BTreeMap::new();
    match &input.data {
        RegData::Handle(handle) => {
            for path in &project.field_paths {
                let value = handle_field(handle, path).ok_or_else(|| {
                    fail_rejected(ErrorCode::FieldRedacted, "PROJECT field path missing")
                })?;
                out.insert(path.clone(), value);
            }
        }
        RegData::HandleSet(handles) => {
            if let Some(first) = handles.first() {
                for path in &project.field_paths {
                    if path == "set_count" {
                        continue;
                    }
                    let value = handle_field(first, path).ok_or_else(|| {
                        fail_rejected(ErrorCode::FieldRedacted, "PROJECT field path missing")
                    })?;
                    out.insert(path.clone(), value);
                }
            }
            out.insert(
                "set_count".to_string(),
                Value {
                    v: Some(V::I64(handles.len() as i64)),
                },
            );
        }
        RegData::Struct(values) => {
            for path in &project.field_paths {
                let value = values
                    .get(path)
                    .cloned()
                    .ok_or_else(|| fail_rejected(ErrorCode::FieldRedacted, "PROJECT field missing"))?;
                out.insert(path.clone(), value);
            }
        }
    }

    Ok(RegValue {
        data: RegData::Struct(out),
        provenance: input.provenance.clone(),
        taint: input.taint.clone(),
        min_trust: input.min_trust,
        join_depth: input.join_depth,
        source: SourceKind::Project,
    })
}

fn exec_assert(
    assert_op: &OpAssert,
    regs: &BTreeMap<String, RegValue>,
    opts: &ExecuteOptions,
) -> Result<(RegValue, rmvm_proto::VerifiedAssertion), ExecStop> {
    let assertion_type = assertion_type_from_i32(assert_op.assertion_type);
    if assertion_type == AssertionType::Unspecified {
        return Err(fail_rejected(
            ErrorCode::SchemaViolation,
            "ASSERT assertion_type unspecified",
        ));
    }

    let mut fields = BTreeMap::new();
    let mut provenance = BTreeSet::new();
    let mut taint = BTreeSet::new();
    let mut min_trust = TrustTier::Tier4PolicySigned;
    for (field_name, binding) in &assert_op.bindings {
        let reg = regs.get(&binding.reg).ok_or_else(|| {
            fail_rejected(ErrorCode::SchemaViolation, "ASSERT binding register missing")
        })?;
        if !matches!(reg.source, SourceKind::Fetch | SourceKind::Project) {
            return Err(fail_rejected(
                ErrorCode::SchemaViolation,
                "ASSERT bindings must trace to FETCH or PROJECT",
            ));
        }
        let value = extract_binding_value(reg, &binding.field_path)?;
        fields.insert(field_name.clone(), value);
        provenance.extend(reg.provenance.iter().cloned());
        taint.extend(reg.taint.iter().copied());
        min_trust = min_tier(min_trust, reg.min_trust);
    }
    if fields.is_empty() {
        min_trust = TrustTier::Unspecified;
    }

    if assertion_requires_tier2(assertion_type) && trust_rank(min_trust) < trust_rank(TrustTier::Tier2Verified) {
        return Err(fail_rejected(
            ErrorCode::UntrustedProvenance,
            "assertion requires trust tier >= TIER_2",
        ));
    }
    if assertion_requires_tier2(assertion_type) && has_untrusted_taint(&taint) {
        return Err(fail_rejected(
            ErrorCode::DataLeakPrevention,
            "policy-impacting assertion cannot sink web-untrusted tainted data",
        ));
    }
    if !opts.degraded_mode
        && provenance
            .iter()
            .any(|handle_ref| opts.broken_lineage_handles.contains(handle_ref))
    {
        return Err(fail_rejected(
            ErrorCode::ErrorBrokenLineage,
            "ASSERT references broken-lineage provenance",
        ));
    }

    let citations = canonicalize_citations(&assert_op.citations, &provenance, opts)?;
    let assertion = rmvm_proto::VerifiedAssertion {
        assertion_type: assertion_type as i32,
        fields: fields.clone(),
        citations,
    };
    let reg = RegValue {
        data: RegData::Struct(fields),
        provenance,
        taint,
        min_trust,
        join_depth: 0,
        source: SourceKind::Assert,
    };
    Ok((reg, assertion))
}

fn canonicalize_citations(
    citations: &[CitationRef],
    provenance: &BTreeSet<String>,
    opts: &ExecuteOptions,
) -> Result<Vec<CanonicalCitation>, ExecStop> {
    let mut unique = BTreeSet::new();
    if citations.is_empty() {
        for handle_ref in provenance {
            if !opts.degraded_mode && opts.broken_lineage_handles.contains(handle_ref) {
                return Err(fail_rejected(
                    ErrorCode::ErrorBrokenLineage,
                    "citation includes broken-lineage handle",
                ));
            }
            unique.insert(format!("handle:{handle_ref}"));
        }
    } else {
        for citation in citations {
            match citation.cite.as_ref() {
                Some(Cite::HandleRef(handle_ref)) => {
                    if !opts.degraded_mode && opts.broken_lineage_handles.contains(handle_ref) {
                        return Err(fail_rejected(
                            ErrorCode::ErrorBrokenLineage,
                            "citation includes broken-lineage handle",
                        ));
                    }
                    unique.insert(format!("handle:{handle_ref}"));
                }
                Some(Cite::AnchorRef(anchor_ref)) => {
                    unique.insert(anchor_ref.clone());
                }
                None => {
                    return Err(fail_rejected(
                        ErrorCode::SchemaViolation,
                        "citation is missing variant",
                    ));
                }
            }
        }
    }
    Ok(unique
        .into_iter()
        .map(|anchor_digest| CanonicalCitation { anchor_digest })
        .collect())
}

fn render_output(
    outputs: &[OutputSpec],
    regs: &BTreeMap<String, RegValue>,
    assertions: &[rmvm_proto::VerifiedAssertion],
    opts: &ExecuteOptions,
) -> Result<RenderedOutput, ExecStop> {
    let mut verified_blocks = Vec::new();
    if assertions.is_empty() {
        for output in outputs {
            if let Some(reg) = regs.get(&output.reg) {
                verified_blocks.push(format!("{}={}", output.reg, reg_to_string(reg)));
            }
        }
    } else {
        for assertion in assertions {
            let at = assertion_type_from_i32(assertion.assertion_type).as_str_name();
            let fields = assertion
                .fields
                .iter()
                .map(|(k, v)| format!("{k}={}", value_to_string(v)))
                .collect::<Vec<_>>()
                .join(", ");
            verified_blocks.push(format!("{at}: {fields}"));
        }
    }

    let mut narrative_blocks = Vec::new();
    for template in &opts.narrative_templates {
        if !valid_narrative_template(template) {
            return Err(fail_rejected(
                ErrorCode::DataLeakPrevention,
                "narrative template violates grammar guard",
            ));
        }
        narrative_blocks.push(render_narrative_template(template, assertions)?);
    }
    Ok(RenderedOutput {
        verified_blocks,
        narrative_blocks,
    })
}

fn render_narrative_template(
    template: &str,
    assertions: &[rmvm_proto::VerifiedAssertion],
) -> Result<String, ExecStop> {
    let binding_re = Regex::new(r"\{A\[(\d+)\]\.([a-zA-Z_][a-zA-Z0-9_]*)\}").expect("regex");
    let mut rendered = String::new();
    let mut last = 0usize;
    for capture in binding_re.captures_iter(template) {
        let m = capture
            .get(0)
            .ok_or_else(|| fail_rejected(ErrorCode::SchemaViolation, "narrative binding parse failure"))?;
        rendered.push_str(&template[last..m.start()]);
        let idx = capture
            .get(1)
            .and_then(|g| g.as_str().parse::<usize>().ok())
            .ok_or_else(|| fail_rejected(ErrorCode::SchemaViolation, "invalid narrative assertion index"))?;
        let field = capture
            .get(2)
            .map(|g| g.as_str())
            .ok_or_else(|| fail_rejected(ErrorCode::SchemaViolation, "invalid narrative field"))?;
        let value = assertions
            .get(idx)
            .and_then(|a| a.fields.get(field))
            .ok_or_else(|| fail_rejected(ErrorCode::FieldRedacted, "narrative binding field missing"))?;
        rendered.push_str(&value_to_string(value));
        last = m.end();
    }
    rendered.push_str(&template[last..]);
    Ok(rendered)
}

fn valid_narrative_template(template: &str) -> bool {
    let binding_re = Regex::new(r"\{A\[\d+\]\.[a-zA-Z_][a-zA-Z0-9_]*\}").expect("regex");
    let macro_re = Regex::new(r"\{\{macro\.[a-zA-Z_][a-zA-Z0-9_.-]*\}\}").expect("regex");
    let remaining = macro_re
        .replace_all(&binding_re.replace_all(template, ""), "")
        .to_string();
    Regex::new(r"^[a-z\s\.,!\?]*$")
        .expect("regex")
        .is_match(&remaining)
}

fn build_proof(
    assertions: &[rmvm_proto::VerifiedAssertion],
    trace_tokens: &[String],
) -> AssertionMerkleProof {
    let leaves: Vec<[u8; 32]> = assertions
        .iter()
        .map(|a| {
            let mut out = [0u8; 32];
            let hash = hex::decode(canonical_assertion_hash(a)).expect("hash decode");
            out.copy_from_slice(&hash);
            out
        })
        .collect();
    let (semantic_root, inclusion) = merkle_root_and_inclusion(&leaves);
    let trace_root = sha256_hex(trace_tokens.join("\n").as_bytes());
    AssertionMerkleProof {
        semantic_root,
        trace_root,
        inclusion,
    }
}

fn merkle_root_and_inclusion(leaves: &[[u8; 32]]) -> (String, Vec<InclusionProof>) {
    if leaves.is_empty() {
        return (sha256_hex(&[]), Vec::new());
    }
    let mut levels: Vec<Vec<[u8; 32]>> = vec![leaves.to_vec()];
    while levels.last().map(|l| l.len()).unwrap_or(0) > 1 {
        let prev = levels.last().expect("level");
        let mut next = Vec::new();
        let mut i = 0usize;
        while i < prev.len() {
            let left = prev[i];
            let right = if i + 1 < prev.len() { prev[i + 1] } else { prev[i] };
            next.push(hash_pair(left, right));
            i += 2;
        }
        levels.push(next);
    }
    let root = levels.last().expect("root")[0];
    let mut inclusion = Vec::new();
    for idx in 0..leaves.len() {
        let mut siblings = Vec::new();
        let mut cur = idx;
        for level in &levels[..levels.len() - 1] {
            let sib_idx = if cur % 2 == 0 {
                if cur + 1 < level.len() { cur + 1 } else { cur }
            } else {
                cur - 1
            };
            siblings.push(hex::encode(level[sib_idx]));
            cur /= 2;
        }
        inclusion.push(InclusionProof {
            assertion_index: idx as u32,
            sibling_hashes: siblings,
        });
    }
    (hex::encode(root), inclusion)
}

fn hash_pair(left: [u8; 32], right: [u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(left);
    hasher.update(right);
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn extract_binding_value(reg: &RegValue, path: &str) -> Result<Value, ExecStop> {
    match &reg.data {
        RegData::Handle(handle) => handle_field(handle, path)
            .ok_or_else(|| fail_rejected(ErrorCode::FieldRedacted, "binding path missing on handle")),
        RegData::Struct(values) => values
            .get(path)
            .cloned()
            .ok_or_else(|| fail_rejected(ErrorCode::FieldRedacted, "binding path missing in struct")),
        RegData::HandleSet(_) => Err(fail_rejected(
            ErrorCode::TypeMismatch,
            "cannot bind directly from handle set",
        )),
    }
}

fn as_handles(reg: &RegValue) -> Result<Vec<HandleRef>, ExecStop> {
    match &reg.data {
        RegData::Handle(handle) => Ok(vec![handle.clone()]),
        RegData::HandleSet(handles) => Ok(handles.clone()),
        RegData::Struct(_) => Err(fail_rejected(
            ErrorCode::TypeMismatch,
            "operation requires HANDLE or HANDLE_SET",
        )),
    }
}

fn matches_filter(
    handle: &HandleRef,
    filter_ref: &str,
    params: &BTreeMap<String, Value>,
) -> Result<bool, ExecStop> {
    match filter_ref {
        "identity" => Ok(true),
        "by_subject" => {
            let subject = as_string(params.get("subject")).ok_or_else(|| {
                fail_rejected(ErrorCode::SchemaViolation, "by_subject requires subject string")
            })?;
            Ok(handle.meta.as_ref().map(|m| m.subject.as_str()) == Some(subject.as_str()))
        }
        "by_type" => {
            let type_id = as_string(params.get("type_id")).ok_or_else(|| {
                fail_rejected(ErrorCode::SchemaViolation, "by_type requires type_id string")
            })?;
            Ok(handle.type_id == type_id)
        }
        "by_scope" => {
            let scope = as_string_or_enum(params.get("scope")).ok_or_else(|| {
                fail_rejected(ErrorCode::SchemaViolation, "by_scope requires scope enum/string")
            })?;
            let actual = handle
                .meta
                .as_ref()
                .map(|m| {
                    rmvm_proto::Scope::try_from(m.scope)
                        .unwrap_or(rmvm_proto::Scope::Unspecified)
                        .as_str_name()
                })
                .unwrap_or("");
            Ok(actual == scope)
        }
        "trust_at_least" => {
            let expected = params
                .get("tier")
                .and_then(value_to_tier_rank)
                .ok_or_else(|| fail_rejected(ErrorCode::SchemaViolation, "trust_at_least requires tier"))?;
            let actual = handle
                .meta
                .as_ref()
                .map(|m| trust_rank(trust_tier_from_i32(m.trust_tier)))
                .unwrap_or(0);
            Ok(actual >= expected)
        }
        _ => Err(fail_rejected(ErrorCode::SchemaViolation, "unknown filter_ref")),
    }
}

fn selector_match(handle: &HandleRef, params: &BTreeMap<String, Value>) -> bool {
    for (name, value) in params {
        match name.as_str() {
            "subject" => {
                if handle.meta.as_ref().map(|m| m.subject.as_str()) != as_string(Some(value)).as_deref() {
                    return false;
                }
            }
            "type_id" => {
                if Some(handle.type_id.as_str()) != as_string(Some(value)).as_deref() {
                    return false;
                }
            }
            "scope" => {
                let expected = as_string_or_enum(Some(value));
                let actual = handle.meta.as_ref().map(|m| {
                    rmvm_proto::Scope::try_from(m.scope)
                        .unwrap_or(rmvm_proto::Scope::Unspecified)
                        .as_str_name()
                        .to_string()
                });
                if actual != expected {
                    return false;
                }
            }
            "availability" => {
                let expected = as_string_or_enum(Some(value));
                let actual = Some(availability_from_i32(handle.availability).as_str_name().to_string());
                if actual != expected {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

fn validate_param_specs(
    specs: &[ParamSpec],
    params: &BTreeMap<String, Value>,
) -> Result<(), ExecStop> {
    let spec_by_name: BTreeMap<&str, &ParamSpec> = specs.iter().map(|s| (s.name.as_str(), s)).collect();
    for (name, value) in params {
        let spec = spec_by_name
            .get(name.as_str())
            .copied()
            .ok_or_else(|| fail_rejected(ErrorCode::SchemaViolation, "param not in selector spec"))?;
        if !value_matches_type(value, spec) {
            return Err(fail_rejected(ErrorCode::TypeMismatch, "param type mismatch"));
        }
    }
    Ok(())
}

fn value_matches_type(value: &Value, spec: &ParamSpec) -> bool {
    match param_type_from_i32(spec.r#type) {
        ParamType::ParamString => matches!(value.v, Some(V::S(_))),
        ParamType::ParamBool => matches!(value.v, Some(V::B(_))),
        ParamType::ParamInt64 => matches!(value.v, Some(V::I64(_))),
        ParamType::ParamFloat64 => matches!(value.v, Some(V::F64(_))),
        ParamType::ParamTimestamp => matches!(value.v, Some(V::Ts(_))),
        ParamType::ParamEnum | ParamType::ParamScope => {
            if let Some(V::E(ref e)) = value.v {
                spec.enum_values.is_empty() || spec.enum_values.contains(e)
            } else {
                false
            }
        }
        ParamType::Unspecified => false,
    }
}

fn estimate_selector_rows(
    selector: &SelectorRef,
    manifest: &PublicManifest,
    budget: &NormalizedBudget,
) -> u32 {
    match selector_return_from_i32(selector.return_type) {
        SelectorReturn::ReturnHandle => 1,
        SelectorReturn::ReturnHandleSet => manifest.handles.len().min(budget.max_fanout as usize) as u32,
        SelectorReturn::ReturnStruct | SelectorReturn::Unspecified => 1,
    }
}

fn handles_index(handles: &[HandleRef]) -> BTreeMap<String, HandleRef> {
    handles.iter().map(|h| (h.r#ref.clone(), h.clone())).collect()
}

fn selectors_index(selectors: &[SelectorRef]) -> BTreeMap<String, SelectorRef> {
    selectors.iter().map(|s| (s.sel.clone(), s.clone())).collect()
}

fn normalize_budget(budget: Option<&PlanBudget>) -> NormalizedBudget {
    let max_ops = budget.and_then(|b| non_zero(b.max_ops)).unwrap_or(128);
    let max_join_depth = budget.and_then(|b| non_zero(b.max_join_depth)).unwrap_or(3);
    let max_fanout = budget.and_then(|b| non_zero(b.max_fanout)).unwrap_or(64);
    let max_total_cost = budget
        .map(|b| if b.max_total_cost <= 0.0 { 512.0 } else { b.max_total_cost })
        .unwrap_or(512.0);
    NormalizedBudget {
        max_ops,
        max_join_depth,
        max_fanout,
        max_total_cost,
    }
}

fn non_zero(v: u32) -> Option<u32> {
    if v == 0 { None } else { Some(v) }
}

fn trust_rank(tier: TrustTier) -> i32 {
    tier as i32
}

fn min_tier(a: TrustTier, b: TrustTier) -> TrustTier {
    if trust_rank(a) <= trust_rank(b) { a } else { b }
}

fn timestamp_score(ts: &prost_types::Timestamp) -> i64 {
    ts.seconds.saturating_mul(1_000_000_000).saturating_add(ts.nanos as i64)
}

fn assertion_requires_tier2(t: AssertionType) -> bool {
    matches!(
        t,
        AssertionType::AssertUserPreference
            | AssertionType::AssertDecision
            | AssertionType::AssertProcedure
    )
}

fn has_untrusted_taint(taint: &BTreeSet<i32>) -> bool {
    taint.contains(&(TaintClass::TaintWebUntrusted as i32))
        || taint.contains(&(TaintClass::TaintMixed as i32))
}

fn handle_field(handle: &HandleRef, path: &str) -> Option<Value> {
    match path {
        "ref" => Some(Value { v: Some(V::S(handle.r#ref.clone())) }),
        "type_id" => Some(Value { v: Some(V::S(handle.type_id.clone())) }),
        "signature_summary" => Some(Value { v: Some(V::S(handle.signature_summary.clone())) }),
        "conflict_group_id" => Some(Value { v: Some(V::S(handle.conflict_group_id.clone())) }),
        "availability" => Some(Value {
            v: Some(V::E(availability_from_i32(handle.availability).as_str_name().to_string())),
        }),
        "meta.subject" => handle.meta.as_ref().map(|m| Value { v: Some(V::S(m.subject.clone())) }),
        "meta.predicate_label" => handle
            .meta
            .as_ref()
            .map(|m| Value { v: Some(V::S(m.predicate_label.clone())) }),
        "meta.trust_tier" => handle.meta.as_ref().map(|m| Value {
            v: Some(V::E(trust_tier_from_i32(m.trust_tier).as_str_name().to_string())),
        }),
        "meta.scope" => handle.meta.as_ref().map(|m| Value {
            v: Some(V::E(
                rmvm_proto::Scope::try_from(m.scope)
                    .unwrap_or(rmvm_proto::Scope::Unspecified)
                    .as_str_name()
                    .to_string(),
            )),
        }),
        "meta.temporal.open_end" => handle
            .meta
            .as_ref()
            .and_then(|m| m.temporal.as_ref())
            .map(|t| Value { v: Some(V::B(t.open_end)) }),
        "meta.temporal.valid_from" => handle
            .meta
            .as_ref()
            .and_then(|m| m.temporal.as_ref())
            .and_then(|t| t.valid_from.clone())
            .map(|ts| Value { v: Some(V::Ts(ts)) }),
        "meta.temporal.valid_to" => handle
            .meta
            .as_ref()
            .and_then(|m| m.temporal.as_ref())
            .and_then(|t| t.valid_to.clone())
            .map(|ts| Value { v: Some(V::Ts(ts)) }),
        _ => None,
    }
}

fn as_string(v: Option<&Value>) -> Option<String> {
    match v?.v.as_ref()? {
        V::S(s) => Some(s.clone()),
        _ => None,
    }
}

fn as_string_or_enum(v: Option<&Value>) -> Option<String> {
    match v?.v.as_ref()? {
        V::S(s) | V::E(s) => Some(s.clone()),
        _ => None,
    }
}

fn value_to_tier_rank(value: &Value) -> Option<i32> {
    match value.v.as_ref()? {
        V::I64(v) => Some(*v as i32),
        V::E(e) => Some(trust_rank(trust_tier_from_i32(parse_enum_i32(e)))),
        _ => None,
    }
}

fn parse_enum_i32(name: &str) -> i32 {
    match name {
        "TIER_0_QUARANTINED" => TrustTier::Tier0Quarantined as i32,
        "TIER_1_ASSERTED" => TrustTier::Tier1Asserted as i32,
        "TIER_2_VERIFIED" => TrustTier::Tier2Verified as i32,
        "TIER_3_CONFIRMED" => TrustTier::Tier3Confirmed as i32,
        "TIER_4_POLICY_SIGNED" => TrustTier::Tier4PolicySigned as i32,
        _ => TrustTier::Unspecified as i32,
    }
}

fn value_to_string(value: &Value) -> String {
    match value.v.as_ref() {
        Some(V::S(v)) => v.clone(),
        Some(V::B(v)) => v.to_string(),
        Some(V::I64(v)) => v.to_string(),
        Some(V::F64(v)) => v.to_string(),
        Some(V::Ts(v)) => format!("{}:{}", v.seconds, v.nanos),
        Some(V::E(v)) => v.clone(),
        None => "<null>".to_string(),
    }
}

fn reg_to_string(reg: &RegValue) -> String {
    match &reg.data {
        RegData::Handle(h) => format!("HANDLE({})", h.r#ref),
        RegData::HandleSet(set) => format!(
            "HANDLE_SET([{}])",
            set.iter().map(|h| h.r#ref.as_str()).collect::<Vec<_>>().join(",")
        ),
        RegData::Struct(fields) => format!(
            "STRUCT({})",
            fields
                .iter()
                .map(|(k, v)| format!("{k}:{}", value_to_string(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn fail_rejected(code: ErrorCode, message: impl Into<String>) -> ExecStop {
    ExecStop::Failure(KernelFailure {
        status: ExecutionStatus::Rejected,
        code,
        message: message.into(),
        hints: Vec::new(),
    })
}
