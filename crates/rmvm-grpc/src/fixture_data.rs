use std::collections::BTreeMap;

use rmvm_proto::cortex::rmvm::v3_1::step::Op;
use rmvm_proto::cortex::rmvm::v3_1::value::V;
use rmvm_proto::{
    AssertionType, ContextVar, ExecuteRequest, HandleAvailability, HandleMeta, HandleRef, OpAssert,
    OpFetch, OpProject, OutputSpec, ParamSpec, ParamType, PlanBudget, PublicManifest, RmvmPlan,
    Scope, SelectorRef, SelectorReturn, Step, TemporalBound, TrustTier, Value, ValueRef,
};

pub fn golden_execute_request() -> ExecuteRequest {
    let manifest = PublicManifest {
        request_id: "golden-req-001".to_string(),
        handles: vec![HandleRef {
            r#ref: "H0".to_string(),
            type_id: "normative.preference".to_string(),
            availability: HandleAvailability::Ready as i32,
            meta: Some(HandleMeta {
                subject: "user:vinz".to_string(),
                predicate_label: "prefers_beverage".to_string(),
                trust_tier: TrustTier::Tier3Confirmed as i32,
                taint: Vec::new(),
                temporal: Some(TemporalBound {
                    valid_from: None,
                    valid_to: None,
                    open_end: true,
                }),
                scope: Scope::Global as i32,
            }),
            signature_summary: "User stated a hot drink preference".to_string(),
            conflict_group_id: "conflict:prefers_beverage:user:vinz:global".to_string(),
        }],
        selectors: vec![SelectorRef {
            sel: "S0".to_string(),
            description: "Find preferences by subject".to_string(),
            params: vec![ParamSpec {
                name: "subject".to_string(),
                r#type: ParamType::ParamString as i32,
                enum_values: Vec::new(),
            }],
            cost_weight: 1.25,
            return_type: SelectorReturn::ReturnHandleSet as i32,
        }],
        context: vec![ContextVar {
            name: "scope".to_string(),
            value: Some(Value {
                v: Some(V::E("SCOPE_GLOBAL".to_string())),
            }),
        }],
        budget: Some(PlanBudget {
            max_ops: 128,
            max_join_depth: 3,
            max_fanout: 64,
            max_total_cost: 256.0,
        }),
    };

    let mut bindings = BTreeMap::new();
    bindings.insert(
        "subject".to_string(),
        ValueRef {
            reg: "r1".to_string(),
            field_path: "meta.subject".to_string(),
        },
    );

    let plan = RmvmPlan {
        request_id: "golden-req-001".to_string(),
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
        ],
        outputs: vec![OutputSpec {
            reg: "r2".to_string(),
        }],
    };

    ExecuteRequest {
        manifest: Some(manifest),
        plan: Some(plan),
    }
}
