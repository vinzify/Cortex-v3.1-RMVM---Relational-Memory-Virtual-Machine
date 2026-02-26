from __future__ import annotations

from dataclasses import dataclass, field
from typing import Dict, List, Literal, Optional, Union

Scope = Literal[
    "SCOPE_UNSPECIFIED",
    "SCOPE_SESSION",
    "SCOPE_PROJECT",
    "SCOPE_PERSON",
    "SCOPE_ORG",
    "SCOPE_GLOBAL",
]

AssertionType = Literal[
    "ASSERTION_TYPE_UNSPECIFIED",
    "ASSERT_USER_PREFERENCE",
    "ASSERT_WORLD_FACT",
    "ASSERT_DECISION",
    "ASSERT_PROCEDURE",
    "ASSERT_CONFLICT_EXPLANATION",
]

PrimitiveValue = Union[str, bool, int, float]


@dataclass(slots=True)
class ManifestHandle:
    ref: str
    type_id: str
    availability: str
    subject: Optional[str] = None
    predicate_label: Optional[str] = None
    signature_summary: Optional[str] = None


@dataclass(slots=True)
class Manifest:
    request_id: str
    handles: List[ManifestHandle]
    selector_refs: List[str]
    raw: object


@dataclass(slots=True)
class FetchOp:
    handle_ref: str


@dataclass(slots=True)
class ApplySelectorOp:
    selector_ref: str
    params: Dict[str, PrimitiveValue] = field(default_factory=dict)


@dataclass(slots=True)
class ResolveOp:
    in_reg: str
    policy_id: str = ""


@dataclass(slots=True)
class FilterOp:
    in_reg: str
    filter_ref: str
    params: Dict[str, PrimitiveValue] = field(default_factory=dict)


@dataclass(slots=True)
class JoinOp:
    left_reg: str
    right_reg: str
    edge_type: str


@dataclass(slots=True)
class ProjectOp:
    in_reg: str
    field_paths: List[str]


@dataclass(slots=True)
class AssertBinding:
    reg: str
    field_path: str


@dataclass(slots=True)
class Citation:
    handle_ref: Optional[str] = None
    anchor_ref: Optional[str] = None


@dataclass(slots=True)
class AssertOp:
    assertion_type: AssertionType
    bindings: Dict[str, AssertBinding]
    citations: List[Citation] = field(default_factory=list)


PlanOperation = Union[FetchOp, ApplySelectorOp, ResolveOp, FilterOp, JoinOp, ProjectOp, AssertOp]


@dataclass(slots=True)
class PlanStep:
    out: str
    op: PlanOperation


@dataclass(slots=True)
class PlanInput:
    request_id: str
    steps: List[PlanStep]
    outputs: List[str]


@dataclass(slots=True)
class AppendEventResult:
    event_id: str
    handle_refs: List[str]


@dataclass(slots=True)
class ExecutePlanResult:
    status: str
    verified_blocks: List[str]
    narrative_blocks: List[str]
    semantic_root: Optional[str] = None
    trace_root: Optional[str] = None
    stall_handle_ref: Optional[str] = None
    stall_availability: Optional[str] = None


@dataclass(slots=True)
class ForgetResult:
    status: str
    verified_blocks: List[str]
