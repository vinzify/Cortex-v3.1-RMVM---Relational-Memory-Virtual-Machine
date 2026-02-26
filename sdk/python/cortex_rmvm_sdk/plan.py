from __future__ import annotations

from .types import (
    ApplySelectorOp,
    AssertOp,
    FilterOp,
    JoinOp,
    Manifest,
    PlanInput,
    ProjectOp,
    ResolveOp,
)


def build_plan_only_prompt(user_message: str, manifest: Manifest) -> str:
    handles = ", ".join(h.ref for h in manifest.handles)
    selectors = ", ".join(manifest.selector_refs)
    return "\n".join(
        [
            "Return plan JSON only. Do not include prose.",
            "Generate a valid RMVMPlan with requestId, steps, outputs.",
            f"User message: {user_message}",
            f"Allowed handle refs: [{handles}]",
            f"Allowed selector refs: [{selectors}]",
            "Every fetch.handleRef must be from allowed handle refs.",
            "Every applySelector.selectorRef must be from allowed selector refs.",
        ]
    )


def validate_plan_against_manifest(plan: PlanInput, manifest: Manifest) -> None:
    handle_refs = {h.ref for h in manifest.handles}
    selector_refs = set(manifest.selector_refs)
    regs: set[str] = set()

    for step in plan.steps:
        if not step.out:
            raise ValueError("invalid plan: step.out is required")
        if step.out in regs:
            raise ValueError(f"invalid plan: register redefined ({step.out})")

        op = step.op
        if hasattr(op, "handle_ref"):
            if op.handle_ref not in handle_refs:
                raise ValueError(f"invalid plan: unknown handle ref {op.handle_ref}")
        elif isinstance(op, ApplySelectorOp):
            if op.selector_ref not in selector_refs:
                raise ValueError(f"invalid plan: unknown selector ref {op.selector_ref}")
        elif isinstance(op, (ResolveOp, FilterOp, ProjectOp)):
            if op.in_reg not in regs:
                raise ValueError(f"invalid plan: input register not defined ({op.in_reg})")
        elif isinstance(op, JoinOp):
            if op.left_reg not in regs or op.right_reg not in regs:
                raise ValueError("invalid plan: join registers not defined")
        elif isinstance(op, AssertOp):
            for binding in op.bindings.values():
                if binding.reg not in regs:
                    raise ValueError(
                        f"invalid plan: assert binding register not defined ({binding.reg})"
                    )

        regs.add(step.out)
