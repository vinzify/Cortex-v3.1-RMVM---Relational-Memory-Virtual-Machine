from __future__ import annotations

from typing import Dict

import grpc

from .generated import (
    cortex_rmvm_v3_1_pb2 as pb2,
    cortex_rmvm_v3_1_service_pb2 as service_pb2,
    cortex_rmvm_v3_1_service_pb2_grpc as service_pb2_grpc,
)
from .plan import validate_plan_against_manifest
from .types import (
    AppendEventResult,
    ApplySelectorOp,
    AssertOp,
    ExecutePlanResult,
    FetchOp,
    FilterOp,
    ForgetResult,
    JoinOp,
    Manifest,
    ManifestHandle,
    PlanInput,
    PrimitiveValue,
    ProjectOp,
    ResolveOp,
)


class CortexRmvmClient:
    def __init__(self, target: str) -> None:
        self._channel = grpc.insecure_channel(target)
        self._stub = service_pb2_grpc.RmvmExecutorStub(self._channel)

    def append_event(
        self, request_id: str, subject: str, text: str, scope: str = "SCOPE_GLOBAL"
    ) -> AppendEventResult:
        req = service_pb2.AppendEventRequest(
            request_id=request_id,
            subject=subject,
            text=text,
            scope=getattr(pb2, scope),
        )
        resp = self._stub.AppendEvent(req)
        return AppendEventResult(event_id=resp.event_id, handle_refs=list(resp.handle_refs))

    def get_manifest(self, request_id: str) -> Manifest:
        resp = self._stub.GetManifest(service_pb2.GetManifestRequest(request_id=request_id))
        manifest = resp.manifest
        handles = [
            ManifestHandle(
                ref=h.ref,
                type_id=h.type_id,
                availability=pb2.HandleAvailability.Name(h.availability),
                subject=h.meta.subject if h.HasField("meta") else None,
                predicate_label=h.meta.predicate_label if h.HasField("meta") else None,
                signature_summary=h.signature_summary or None,
            )
            for h in manifest.handles
        ]
        return Manifest(
            request_id=manifest.request_id,
            handles=handles,
            selector_refs=[s.sel for s in manifest.selectors],
            raw=manifest,
        )

    def execute_plan(self, request_id: str, manifest: Manifest, plan: PlanInput) -> ExecutePlanResult:
        validate_plan_against_manifest(plan, manifest)
        req = pb2.ExecuteRequest(manifest=manifest.raw, plan=_plan_to_proto(plan))
        resp = self._stub.Execute(req)
        proof = resp.proof if resp.HasField("proof") else None
        rendered = resp.rendered if resp.HasField("rendered") else pb2.RenderedOutput()
        stall = resp.stall if resp.HasField("stall") else None
        return ExecutePlanResult(
            status=pb2.ExecutionStatus.Name(resp.status),
            verified_blocks=list(rendered.verified_blocks),
            narrative_blocks=list(rendered.narrative_blocks),
            semantic_root=proof.semantic_root if proof else None,
            trace_root=proof.trace_root if proof else None,
            stall_handle_ref=stall.handle_ref if stall else None,
            stall_availability=pb2.HandleAvailability.Name(stall.availability) if stall else None,
        )

    def forget(
        self,
        request_id: str,
        subject: str,
        predicate_label: str,
        scope: str = "SCOPE_GLOBAL",
        reason: str = "",
    ) -> ForgetResult:
        req = service_pb2.ForgetRequest(
            request_id=request_id,
            subject=subject,
            predicate_label=predicate_label,
            scope=getattr(pb2, scope),
            reason=reason,
        )
        resp = self._stub.Forget(req)
        rendered = resp.rendered if resp.HasField("rendered") else pb2.RenderedOutput()
        return ForgetResult(
            status=pb2.ExecutionStatus.Name(resp.status),
            verified_blocks=list(rendered.verified_blocks),
        )

    def close(self) -> None:
        self._channel.close()


def _plan_to_proto(plan: PlanInput) -> pb2.RMVMPlan:
    out = pb2.RMVMPlan(request_id=plan.request_id)
    for step in plan.steps:
        step_msg = out.steps.add()
        step_msg.out = step.out
        op = step.op
        if isinstance(op, FetchOp):
            step_msg.fetch.handle_ref = op.handle_ref
        elif isinstance(op, ApplySelectorOp):
            step_msg.apply_selector.selector_ref = op.selector_ref
            _set_param_map(step_msg.apply_selector.params, op.params)
        elif isinstance(op, ResolveOp):
            step_msg.resolve.in_reg = op.in_reg
            step_msg.resolve.policy_id = op.policy_id
        elif isinstance(op, FilterOp):
            step_msg.filter.in_reg = op.in_reg
            step_msg.filter.filter_ref = op.filter_ref
            _set_param_map(step_msg.filter.params, op.params)
        elif isinstance(op, JoinOp):
            step_msg.join.left_reg = op.left_reg
            step_msg.join.right_reg = op.right_reg
            step_msg.join.edge_type = getattr(pb2, op.edge_type)
        elif isinstance(op, ProjectOp):
            step_msg.project.in_reg = op.in_reg
            step_msg.project.field_paths.extend(op.field_paths)
        elif isinstance(op, AssertOp):
            step_msg.assert_op.assertion_type = getattr(pb2, op.assertion_type)
            for key, binding in op.bindings.items():
                step_msg.assert_op.bindings[key].reg = binding.reg
                step_msg.assert_op.bindings[key].field_path = binding.field_path
            for citation in op.citations:
                c = step_msg.assert_op.citations.add()
                if citation.handle_ref:
                    c.handle_ref = citation.handle_ref
                elif citation.anchor_ref:
                    c.anchor_ref = citation.anchor_ref
        else:
            raise TypeError(f"unsupported op type: {type(op)!r}")
    for reg in plan.outputs:
        out.outputs.add(reg=reg)
    return out


def _set_param_map(target: Dict[str, pb2.Value], source: Dict[str, PrimitiveValue]) -> None:
    for key, value in source.items():
        v = target[key]
        if isinstance(value, bool):
            v.b = value
        elif isinstance(value, str):
            v.s = value
        elif isinstance(value, int):
            v.i64 = value
        else:
            v.f64 = float(value)
