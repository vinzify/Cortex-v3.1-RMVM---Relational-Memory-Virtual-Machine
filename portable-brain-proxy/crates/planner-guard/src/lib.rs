use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, anyhow, bail};
use rmvm_proto::cortex::rmvm::v3_1::citation_ref::Cite;
use rmvm_proto::cortex::rmvm::v3_1::step::Op;
use rmvm_proto::cortex::rmvm::v3_1::value::V;
use rmvm_proto::{
    AssertionType, CitationRef, EdgeType, OpApplySelector, OpAssert, OpFetch, OpFilter, OpJoin,
    OpProject, OpResolve, OutputSpec, PublicManifest, RmvmPlan, Step, Value, ValueRef,
};
use serde_json::Value as JsonValue;

pub fn build_plan_only_prompt(user_message: &str, manifest: &PublicManifest) -> String {
    let handles = manifest
        .handles
        .iter()
        .map(|h| h.r#ref.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let selectors = manifest
        .selectors
        .iter()
        .map(|s| s.sel.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    [
        "Return plan JSON only. Do not include prose or markdown.",
        "Use schema: {requestId, steps:[{out, op:{kind,...}}], outputs:[string]}.",
        "Allowed op.kind values: fetch, applySelector, resolve, filter, join, project, assert.",
        "assert bindings shape: bindings.{field} = {reg, fieldPath}.",
        &format!("User message: {user_message}"),
        &format!("Allowed handle refs: [{handles}]"),
        &format!("Allowed selector refs: [{selectors}]"),
        "Every fetch.handleRef must be from allowed handle refs.",
        "Every applySelector.selectorRef must be from allowed selector refs.",
    ]
    .join("\n")
}

pub fn extract_json_object(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Ok(trimmed.to_string());
    }

    if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        let after = after.strip_prefix("json").unwrap_or(after).trim_start();
        if let Some(end) = after.find("```") {
            let body = after[..end].trim();
            if body.starts_with('{') && body.ends_with('}') {
                return Ok(body.to_string());
            }
        }
    }

    let first = trimmed
        .find('{')
        .ok_or_else(|| anyhow!("planner output did not include a JSON object"))?;
    let last = trimmed
        .rfind('}')
        .ok_or_else(|| anyhow!("planner output did not include a JSON object end"))?;
    if first >= last {
        bail!("planner output JSON boundaries are invalid");
    }
    Ok(trimmed[first..=last].to_string())
}

pub fn parse_plan_json(plan_json: &str, fallback_request_id: &str) -> Result<RmvmPlan> {
    let root: JsonValue = serde_json::from_str(plan_json)?;
    let obj = root
        .as_object()
        .ok_or_else(|| anyhow!("plan root must be an object"))?;

    let request_id = get_string(obj, &["requestId", "request_id"])
        .unwrap_or_else(|| fallback_request_id.to_string());

    let steps_v = obj
        .get("steps")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("plan.steps must be an array"))?;

    let mut steps = Vec::with_capacity(steps_v.len());
    for step_v in steps_v {
        let step_obj = step_v
            .as_object()
            .ok_or_else(|| anyhow!("plan.steps entries must be objects"))?;
        let out = get_string(step_obj, &["out"]).ok_or_else(|| anyhow!("plan step missing out"))?;

        let op = if let Some(unified_op) = step_obj.get("op") {
            parse_unified_op(unified_op)?
        } else {
            parse_proto_style_op(step_obj)?
        };

        steps.push(Step { out, op: Some(op) });
    }

    let outputs = parse_outputs(obj.get("outputs"))?;

    Ok(RmvmPlan {
        request_id,
        steps,
        outputs,
    })
}

pub fn validate_plan_against_manifest(plan: &RmvmPlan, manifest: &PublicManifest) -> Result<()> {
    let handle_refs = manifest
        .handles
        .iter()
        .map(|h| h.r#ref.clone())
        .collect::<BTreeSet<_>>();
    let selector_refs = manifest
        .selectors
        .iter()
        .map(|s| s.sel.clone())
        .collect::<BTreeSet<_>>();

    let mut regs = BTreeSet::new();

    for step in &plan.steps {
        if step.out.trim().is_empty() {
            bail!("invalid plan: step.out is required");
        }
        if !regs.insert(step.out.clone()) {
            bail!("invalid plan: register redefined ({})", step.out);
        }

        let op = step
            .op
            .as_ref()
            .ok_or_else(|| anyhow!("invalid plan: step.op is required"))?;
        match op {
            Op::Fetch(fetch) => {
                if !handle_refs.contains(&fetch.handle_ref) {
                    bail!("invalid plan: unknown handle ref {}", fetch.handle_ref);
                }
            }
            Op::ApplySelector(sel) => {
                if !selector_refs.contains(&sel.selector_ref) {
                    bail!("invalid plan: unknown selector ref {}", sel.selector_ref);
                }
            }
            Op::Resolve(resolve) => {
                if !regs.contains(&resolve.in_reg) {
                    bail!(
                        "invalid plan: input register not defined ({})",
                        resolve.in_reg
                    );
                }
            }
            Op::Filter(filter) => {
                if !regs.contains(&filter.in_reg) {
                    bail!(
                        "invalid plan: input register not defined ({})",
                        filter.in_reg
                    );
                }
            }
            Op::Join(join) => {
                if !regs.contains(&join.left_reg) || !regs.contains(&join.right_reg) {
                    bail!("invalid plan: join registers not defined");
                }
            }
            Op::Project(project) => {
                if !regs.contains(&project.in_reg) {
                    bail!(
                        "invalid plan: input register not defined ({})",
                        project.in_reg
                    );
                }
            }
            Op::AssertOp(assertion) => {
                for binding in assertion.bindings.values() {
                    if !regs.contains(&binding.reg) {
                        bail!(
                            "invalid plan: assert binding register not defined ({})",
                            binding.reg
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn deterministic_plan_from_manifest(
    request_id: &str,
    subject: &str,
    manifest: &PublicManifest,
) -> Result<RmvmPlan> {
    if let Some(handle) = manifest.handles.first() {
        let steps = vec![
            Step {
                out: "r0".to_string(),
                op: Some(Op::Fetch(OpFetch {
                    handle_ref: handle.r#ref.clone(),
                })),
            },
            Step {
                out: "r1".to_string(),
                op: Some(Op::Project(OpProject {
                    in_reg: "r0".to_string(),
                    field_paths: vec!["meta.subject".to_string()],
                })),
            },
            Step {
                out: "r2".to_string(),
                op: Some(Op::AssertOp(OpAssert {
                    assertion_type: AssertionType::AssertWorldFact as i32,
                    bindings: BTreeMap::from([(
                        "subject".to_string(),
                        ValueRef {
                            reg: "r1".to_string(),
                            field_path: "meta.subject".to_string(),
                        },
                    )]),
                    citations: Vec::new(),
                })),
            },
        ];
        return Ok(RmvmPlan {
            request_id: request_id.to_string(),
            steps,
            outputs: vec![OutputSpec {
                reg: "r2".to_string(),
            }],
        });
    }

    let selector = manifest
        .selectors
        .first()
        .ok_or_else(|| anyhow!("manifest has no handles/selectors for deterministic fallback"))?;

    let steps = vec![
        Step {
            out: "r0".to_string(),
            op: Some(Op::ApplySelector(OpApplySelector {
                selector_ref: selector.sel.clone(),
                params: BTreeMap::from([(
                    "subject".to_string(),
                    Value {
                        v: Some(V::S(subject.to_string())),
                    },
                )]),
            })),
        },
        Step {
            out: "r1".to_string(),
            op: Some(Op::Project(OpProject {
                in_reg: "r0".to_string(),
                field_paths: vec!["set_count".to_string()],
            })),
        },
        Step {
            out: "r2".to_string(),
            op: Some(Op::AssertOp(OpAssert {
                assertion_type: AssertionType::AssertWorldFact as i32,
                bindings: BTreeMap::from([(
                    "subject".to_string(),
                    ValueRef {
                        reg: "r1".to_string(),
                        field_path: "set_count".to_string(),
                    },
                )]),
                citations: Vec::new(),
            })),
        },
    ];

    Ok(RmvmPlan {
        request_id: request_id.to_string(),
        steps,
        outputs: vec![OutputSpec {
            reg: "r2".to_string(),
        }],
    })
}

fn parse_outputs(outputs: Option<&JsonValue>) -> Result<Vec<OutputSpec>> {
    let arr = outputs
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("plan.outputs must be an array"))?;
    let mut out = Vec::with_capacity(arr.len());
    for v in arr {
        if let Some(reg) = v.as_str() {
            out.push(OutputSpec {
                reg: reg.to_string(),
            });
            continue;
        }
        let obj = v
            .as_object()
            .ok_or_else(|| anyhow!("plan.outputs entries must be strings or objects"))?;
        let reg = get_string(obj, &["reg"]).ok_or_else(|| anyhow!("output missing reg"))?;
        out.push(OutputSpec { reg });
    }
    Ok(out)
}

fn parse_unified_op(v: &JsonValue) -> Result<Op> {
    let obj = v
        .as_object()
        .ok_or_else(|| anyhow!("step.op must be an object"))?;
    let kind = get_string(obj, &["kind"]).ok_or_else(|| anyhow!("step.op.kind is required"))?;

    match kind.as_str() {
        "fetch" => {
            let handle_ref = get_string(obj, &["handleRef", "handle_ref"])
                .ok_or_else(|| anyhow!("fetch.handleRef is required"))?;
            Ok(Op::Fetch(OpFetch { handle_ref }))
        }
        "applySelector" => {
            let selector_ref = get_string(obj, &["selectorRef", "selector_ref"])
                .ok_or_else(|| anyhow!("applySelector.selectorRef is required"))?;
            let params = parse_param_map(obj.get("params"));
            Ok(Op::ApplySelector(OpApplySelector {
                selector_ref,
                params,
            }))
        }
        "resolve" => {
            let in_reg = get_string(obj, &["inReg", "in_reg"])
                .ok_or_else(|| anyhow!("resolve.inReg is required"))?;
            let policy_id = get_string(obj, &["policyId", "policy_id"]).unwrap_or_default();
            Ok(Op::Resolve(OpResolve { in_reg, policy_id }))
        }
        "filter" => {
            let in_reg = get_string(obj, &["inReg", "in_reg"])
                .ok_or_else(|| anyhow!("filter.inReg is required"))?;
            let filter_ref = get_string(obj, &["filterRef", "filter_ref"])
                .ok_or_else(|| anyhow!("filter.filterRef is required"))?;
            let params = parse_param_map(obj.get("params"));
            Ok(Op::Filter(OpFilter {
                in_reg,
                filter_ref,
                params,
            }))
        }
        "join" => {
            let left_reg = get_string(obj, &["leftReg", "left_reg"])
                .ok_or_else(|| anyhow!("join.leftReg is required"))?;
            let right_reg = get_string(obj, &["rightReg", "right_reg"])
                .ok_or_else(|| anyhow!("join.rightReg is required"))?;
            let edge_type = parse_edge_type(
                get_string(obj, &["edgeType", "edge_type"])
                    .ok_or_else(|| anyhow!("join.edgeType is required"))?
                    .as_str(),
            )?;
            Ok(Op::Join(OpJoin {
                left_reg,
                right_reg,
                edge_type,
            }))
        }
        "project" => {
            let in_reg = get_string(obj, &["inReg", "in_reg"])
                .ok_or_else(|| anyhow!("project.inReg is required"))?;
            let field_paths = obj
                .get("fieldPaths")
                .or_else(|| obj.get("field_paths"))
                .and_then(|v| v.as_array())
                .ok_or_else(|| anyhow!("project.fieldPaths must be an array"))?
                .iter()
                .filter_map(|v| v.as_str())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            Ok(Op::Project(OpProject {
                in_reg,
                field_paths,
            }))
        }
        "assert" => {
            let assertion_type = parse_assertion_type(
                get_string(obj, &["assertionType", "assertion_type"])
                    .ok_or_else(|| anyhow!("assert.assertionType is required"))?
                    .as_str(),
            )?;

            let bindings_obj = obj
                .get("bindings")
                .and_then(|v| v.as_object())
                .ok_or_else(|| anyhow!("assert.bindings must be an object"))?;
            let mut bindings = BTreeMap::new();
            for (k, v) in bindings_obj {
                let b = v
                    .as_object()
                    .ok_or_else(|| anyhow!("assert binding entries must be objects"))?;
                let reg = get_string(b, &["reg"]).ok_or_else(|| anyhow!("binding.reg missing"))?;
                let field_path = get_string(b, &["fieldPath", "field_path"])
                    .ok_or_else(|| anyhow!("binding.fieldPath missing"))?;
                bindings.insert(k.clone(), ValueRef { reg, field_path });
            }

            let mut citations = Vec::new();
            if let Some(c_arr) = obj.get("citations").and_then(|v| v.as_array()) {
                for c in c_arr {
                    let cobj = c
                        .as_object()
                        .ok_or_else(|| anyhow!("citation entries must be objects"))?;
                    if let Some(h) = get_string(cobj, &["handleRef", "handle_ref"]) {
                        citations.push(CitationRef {
                            cite: Some(Cite::HandleRef(h)),
                        });
                    } else if let Some(a) = get_string(cobj, &["anchorRef", "anchor_ref"]) {
                        citations.push(CitationRef {
                            cite: Some(Cite::AnchorRef(a)),
                        });
                    }
                }
            }

            Ok(Op::AssertOp(OpAssert {
                assertion_type,
                bindings,
                citations,
            }))
        }
        _ => bail!("unsupported step.op.kind: {kind}"),
    }
}

fn parse_proto_style_op(obj: &serde_json::Map<String, JsonValue>) -> Result<Op> {
    if let Some(v) = obj.get("fetch") {
        let handle_ref = get_string(
            v.as_object()
                .ok_or_else(|| anyhow!("fetch must be object"))?,
            &["handle_ref", "handleRef"],
        )
        .ok_or_else(|| anyhow!("fetch.handle_ref missing"))?;
        return Ok(Op::Fetch(OpFetch { handle_ref }));
    }
    if let Some(v) = obj.get("apply_selector") {
        let o = v
            .as_object()
            .ok_or_else(|| anyhow!("apply_selector must be object"))?;
        let selector_ref = get_string(o, &["selector_ref", "selectorRef"])
            .ok_or_else(|| anyhow!("apply_selector.selector_ref missing"))?;
        let params = parse_param_map(o.get("params"));
        return Ok(Op::ApplySelector(OpApplySelector {
            selector_ref,
            params,
        }));
    }
    if let Some(v) = obj.get("resolve") {
        let o = v
            .as_object()
            .ok_or_else(|| anyhow!("resolve must be object"))?;
        let in_reg =
            get_string(o, &["in_reg", "inReg"]).ok_or_else(|| anyhow!("resolve.in_reg missing"))?;
        let policy_id = get_string(o, &["policy_id", "policyId"]).unwrap_or_default();
        return Ok(Op::Resolve(OpResolve { in_reg, policy_id }));
    }
    if let Some(v) = obj.get("filter") {
        let o = v
            .as_object()
            .ok_or_else(|| anyhow!("filter must be object"))?;
        let in_reg =
            get_string(o, &["in_reg", "inReg"]).ok_or_else(|| anyhow!("filter.in_reg missing"))?;
        let filter_ref = get_string(o, &["filter_ref", "filterRef"])
            .ok_or_else(|| anyhow!("filter.filter_ref missing"))?;
        let params = parse_param_map(o.get("params"));
        return Ok(Op::Filter(OpFilter {
            in_reg,
            filter_ref,
            params,
        }));
    }
    if let Some(v) = obj.get("join") {
        let o = v
            .as_object()
            .ok_or_else(|| anyhow!("join must be object"))?;
        let left_reg = get_string(o, &["left_reg", "leftReg"])
            .ok_or_else(|| anyhow!("join.left_reg missing"))?;
        let right_reg = get_string(o, &["right_reg", "rightReg"])
            .ok_or_else(|| anyhow!("join.right_reg missing"))?;
        let edge_type = parse_edge_type(
            get_string(o, &["edge_type", "edgeType"])
                .ok_or_else(|| anyhow!("join.edge_type missing"))?
                .as_str(),
        )?;
        return Ok(Op::Join(OpJoin {
            left_reg,
            right_reg,
            edge_type,
        }));
    }
    if let Some(v) = obj.get("project") {
        let o = v
            .as_object()
            .ok_or_else(|| anyhow!("project must be object"))?;
        let in_reg =
            get_string(o, &["in_reg", "inReg"]).ok_or_else(|| anyhow!("project.in_reg missing"))?;
        let field_paths = o
            .get("field_paths")
            .or_else(|| o.get("fieldPaths"))
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("project.field_paths missing"))?
            .iter()
            .filter_map(|v| v.as_str())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        return Ok(Op::Project(OpProject {
            in_reg,
            field_paths,
        }));
    }
    if let Some(v) = obj.get("assert_op") {
        return parse_unified_op(&serde_json::json!({
            "kind": "assert",
            "assertion_type": v.get("assertion_type").cloned().unwrap_or(JsonValue::Null),
            "bindings": v.get("bindings").cloned().unwrap_or(JsonValue::Object(Default::default())),
            "citations": v.get("citations").cloned().unwrap_or(JsonValue::Array(vec![])),
        }));
    }

    bail!("step missing supported operation")
}

fn parse_param_map(value: Option<&JsonValue>) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    let Some(obj) = value.and_then(|v| v.as_object()) else {
        return out;
    };

    for (k, v) in obj {
        if let Some(v) = json_to_rmvm_value(v) {
            out.insert(k.clone(), v);
        }
    }
    out
}

fn json_to_rmvm_value(v: &JsonValue) -> Option<Value> {
    if let Some(obj) = v.as_object() {
        if let Some(s) = obj.get("s").and_then(|x| x.as_str()) {
            return Some(Value {
                v: Some(V::S(s.to_string())),
            });
        }
        if let Some(b) = obj.get("b").and_then(|x| x.as_bool()) {
            return Some(Value { v: Some(V::B(b)) });
        }
        if let Some(i) = obj.get("i64").and_then(|x| x.as_i64()) {
            return Some(Value { v: Some(V::I64(i)) });
        }
        if let Some(f) = obj.get("f64").and_then(|x| x.as_f64()) {
            return Some(Value { v: Some(V::F64(f)) });
        }
        if let Some(e) = obj.get("e").and_then(|x| x.as_str()) {
            return Some(Value {
                v: Some(V::E(e.to_string())),
            });
        }
    }

    if let Some(s) = v.as_str() {
        return Some(Value {
            v: Some(V::S(s.to_string())),
        });
    }
    if let Some(b) = v.as_bool() {
        return Some(Value { v: Some(V::B(b)) });
    }
    if let Some(i) = v.as_i64() {
        return Some(Value { v: Some(V::I64(i)) });
    }
    if let Some(f) = v.as_f64() {
        return Some(Value { v: Some(V::F64(f)) });
    }
    None
}

fn parse_edge_type(s: &str) -> Result<i32> {
    let edge = match s {
        "EDGE_CONFLICTS_WITH" => EdgeType::EdgeConflictsWith,
        "EDGE_SUPERSEDES" => EdgeType::EdgeSupersedes,
        "EDGE_PROVENANCE" => EdgeType::EdgeProvenance,
        "EDGE_SAME_ENTITY" => EdgeType::EdgeSameEntity,
        _ => EdgeType::Unspecified,
    };
    if edge == EdgeType::Unspecified {
        bail!("unsupported edge type: {s}");
    }
    Ok(edge as i32)
}

fn parse_assertion_type(s: &str) -> Result<i32> {
    let at = match s {
        "ASSERT_USER_PREFERENCE" => AssertionType::AssertUserPreference,
        "ASSERT_WORLD_FACT" => AssertionType::AssertWorldFact,
        "ASSERT_DECISION" => AssertionType::AssertDecision,
        "ASSERT_PROCEDURE" => AssertionType::AssertProcedure,
        "ASSERT_CONFLICT_EXPLANATION" => AssertionType::AssertConflictExplanation,
        _ => AssertionType::Unspecified,
    };
    if at == AssertionType::Unspecified {
        bail!("unsupported assertion type: {s}");
    }
    Ok(at as i32)
}

fn get_string(obj: &serde_json::Map<String, JsonValue>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(v) = obj.get(*key).and_then(|v| v.as_str()) {
            return Some(v.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmvm_proto::{
        HandleAvailability, HandleMeta, HandleRef, PlanBudget, Scope, SelectorRef, SelectorReturn,
    };

    fn sample_manifest() -> PublicManifest {
        PublicManifest {
            request_id: "req-1".to_string(),
            handles: vec![HandleRef {
                r#ref: "H1".to_string(),
                type_id: "normative.preference".to_string(),
                availability: HandleAvailability::Ready as i32,
                meta: Some(HandleMeta {
                    subject: "user:demo".to_string(),
                    predicate_label: "prefers_beverage".to_string(),
                    trust_tier: rmvm_proto::TrustTier::Tier3Confirmed as i32,
                    taint: vec![],
                    temporal: None,
                    scope: Scope::Global as i32,
                }),
                signature_summary: "prefers_beverage=tea".to_string(),
                conflict_group_id: "c1".to_string(),
            }],
            selectors: vec![SelectorRef {
                sel: "S0".to_string(),
                description: "selector".to_string(),
                params: vec![],
                cost_weight: 1.0,
                return_type: SelectorReturn::ReturnHandleSet as i32,
            }],
            context: vec![],
            budget: Some(PlanBudget {
                max_ops: 10,
                max_join_depth: 3,
                max_fanout: 10,
                max_total_cost: 10.0,
            }),
        }
    }

    #[test]
    fn deterministic_plan_validates() {
        let manifest = sample_manifest();
        let plan = deterministic_plan_from_manifest("req-1", "user:demo", &manifest).unwrap();
        validate_plan_against_manifest(&plan, &manifest).unwrap();
    }

    #[test]
    fn parse_unified_plan_json() {
        let manifest = sample_manifest();
        let json = r#"{
          "requestId": "req-1",
          "steps": [
            {"out":"r0","op":{"kind":"fetch","handleRef":"H1"}},
            {"out":"r1","op":{"kind":"project","inReg":"r0","fieldPaths":["meta.subject"]}},
            {"out":"r2","op":{"kind":"assert","assertionType":"ASSERT_WORLD_FACT","bindings":{"subject":{"reg":"r1","fieldPath":"meta.subject"}}}}
          ],
          "outputs": ["r2"]
        }"#;

        let plan = parse_plan_json(json, "fallback-req").unwrap();
        validate_plan_against_manifest(&plan, &manifest).unwrap();
        assert_eq!(plan.request_id, "req-1");
    }

    #[test]
    fn extract_json_handles_fence() {
        let s = "```json\n{\"requestId\":\"x\",\"steps\":[],\"outputs\":[]}\n```";
        let out = extract_json_object(s).unwrap();
        assert!(out.starts_with('{'));
    }
}
