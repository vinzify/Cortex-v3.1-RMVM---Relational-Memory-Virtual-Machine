use std::collections::BTreeMap;

use rmvm_kernel::{ExecuteOptions, execute};
use rmvm_proto::cortex::rmvm::v3_1::step::Op;
use rmvm_proto::{
    AssertionType, ContextVar, EdgeType, ErrorCode, ExecuteRequest, ExecutionStatus, HandleAvailability,
    HandleMeta, HandleRef, OpApplySelector, OpAssert, OpFetch, OpJoin, OpProject, OutputSpec,
    ParamSpec, ParamType, PlanBudget, PublicManifest, RmvmPlan, Scope, SelectorRef, SelectorReturn,
    Step, TemporalBound, TrustTier, Value, ValueRef,
};

fn value_s(v: &str) -> Value {
    Value {
        v: Some(rmvm_proto::cortex::rmvm::v3_1::value::V::S(v.to_string())),
    }
}

fn ready_handle(ref_id: &str, trust_tier: TrustTier) -> HandleRef {
    HandleRef {
        r#ref: ref_id.to_string(),
        type_id: "normative.preference".to_string(),
        availability: HandleAvailability::Ready as i32,
        meta: Some(HandleMeta {
            subject: "user:vinz".to_string(),
            predicate_label: "prefers_beverage".to_string(),
            trust_tier: trust_tier as i32,
            taint: Vec::new(),
            temporal: Some(TemporalBound {
                valid_from: None,
                valid_to: None,
                open_end: true,
            }),
            scope: Scope::Global as i32,
        }),
        signature_summary: "test".to_string(),
        conflict_group_id: "conflict:user:vinz:beverage".to_string(),
    }
}

fn base_manifest() -> PublicManifest {
    PublicManifest {
        request_id: "req-1".to_string(),
        handles: vec![ready_handle("H0", TrustTier::Tier3Confirmed)],
        selectors: vec![SelectorRef {
            sel: "S0".to_string(),
            description: "By subject".to_string(),
            params: vec![ParamSpec {
                name: "subject".to_string(),
                r#type: ParamType::ParamString as i32,
                enum_values: Vec::new(),
            }],
            cost_weight: 1.0,
            return_type: SelectorReturn::ReturnHandleSet as i32,
        }],
        context: vec![ContextVar {
            name: "scope".to_string(),
            value: Some(value_s("global")),
        }],
        budget: Some(PlanBudget {
            max_ops: 128,
            max_join_depth: 3,
            max_fanout: 64,
            max_total_cost: 256.0,
        }),
    }
}

#[test]
fn stalls_on_non_ready_fetch() {
    let mut manifest = base_manifest();
    manifest.handles.push(HandleRef {
        availability: HandleAvailability::Offline as i32,
        ..ready_handle("H1", TrustTier::Tier3Confirmed)
    });

    let plan = RmvmPlan {
        request_id: "req-1".to_string(),
        steps: vec![Step {
            out: "r0".to_string(),
            op: Some(Op::Fetch(OpFetch {
                handle_ref: "H1".to_string(),
            })),
        }],
        outputs: Vec::new(),
    };
    let resp = execute(
        ExecuteRequest {
            manifest: Some(manifest),
            plan: Some(plan),
        },
        ExecuteOptions::default(),
    );

    assert_eq!(resp.status, ExecutionStatus::Stall as i32);
    let stall = resp.stall.expect("stall expected");
    assert_eq!(stall.handle_ref, "H1");
}

#[test]
fn rejects_unknown_selector_ref() {
    let manifest = base_manifest();
    let plan = RmvmPlan {
        request_id: "req-1".to_string(),
        steps: vec![Step {
            out: "r0".to_string(),
            op: Some(Op::ApplySelector(OpApplySelector {
                selector_ref: "S404".to_string(),
                params: BTreeMap::new(),
            })),
        }],
        outputs: vec![OutputSpec {
            reg: "r0".to_string(),
        }],
    };
    let resp = execute(
        ExecuteRequest {
            manifest: Some(manifest),
            plan: Some(plan),
        },
        ExecuteOptions::default(),
    );

    assert_eq!(resp.status, ExecutionStatus::Rejected as i32);
    assert_eq!(resp.error.expect("error expected").code, ErrorCode::UnknownSelectorRef as i32);
}

#[test]
fn enforces_trust_gate_for_policy_assertions() {
    let mut manifest = base_manifest();
    manifest.handles = vec![ready_handle("H0", TrustTier::Tier1Asserted)];

    let mut bindings = BTreeMap::new();
    bindings.insert(
        "subject".to_string(),
        ValueRef {
            reg: "r0".to_string(),
            field_path: "meta.subject".to_string(),
        },
    );
    let plan = RmvmPlan {
        request_id: "req-1".to_string(),
        steps: vec![
            Step {
                out: "r0".to_string(),
                op: Some(Op::Fetch(OpFetch {
                    handle_ref: "H0".to_string(),
                })),
            },
            Step {
                out: "r1".to_string(),
                op: Some(Op::AssertOp(OpAssert {
                    assertion_type: AssertionType::AssertDecision as i32,
                    bindings,
                    citations: Vec::new(),
                })),
            },
        ],
        outputs: Vec::new(),
    };
    let resp = execute(
        ExecuteRequest {
            manifest: Some(manifest),
            plan: Some(plan),
        },
        ExecuteOptions::default(),
    );

    assert_eq!(resp.status, ExecutionStatus::Rejected as i32);
    assert_eq!(resp.error.expect("error expected").code, ErrorCode::UntrustedProvenance as i32);
}

#[test]
fn proof_roots_are_deterministic() {
    let manifest = base_manifest();
    let mut bindings = BTreeMap::new();
    bindings.insert(
        "subject".to_string(),
        ValueRef {
            reg: "r1".to_string(),
            field_path: "meta.subject".to_string(),
        },
    );
    let plan = RmvmPlan {
        request_id: "req-1".to_string(),
        steps: vec![
            Step {
                out: "r0".to_string(),
                op: Some(Op::Fetch(OpFetch {
                    handle_ref: "H0".to_string(),
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
                    bindings,
                    citations: Vec::new(),
                })),
            },
            Step {
                out: "r3".to_string(),
                op: Some(Op::Join(OpJoin {
                    left_reg: "r0".to_string(),
                    right_reg: "r0".to_string(),
                    edge_type: EdgeType::EdgeProvenance as i32,
                })),
            },
        ],
        outputs: vec![OutputSpec {
            reg: "r2".to_string(),
        }],
    };

    let req = ExecuteRequest {
        manifest: Some(manifest.clone()),
        plan: Some(plan.clone()),
    };
    let a = execute(req, ExecuteOptions::default());
    let b = execute(
        ExecuteRequest {
            manifest: Some(manifest),
            plan: Some(plan),
        },
        ExecuteOptions::default(),
    );
    assert_eq!(a.status, ExecutionStatus::Ok as i32);
    assert_eq!(b.status, ExecutionStatus::Ok as i32);
    let pa = a.proof.expect("proof expected");
    let pb = b.proof.expect("proof expected");
    assert_eq!(pa.semantic_root, pb.semantic_root);
    assert_eq!(pa.trace_root, pb.trace_root);
}
