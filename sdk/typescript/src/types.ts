export type Scope = "SCOPE_UNSPECIFIED" | "SCOPE_SESSION" | "SCOPE_PROJECT" | "SCOPE_PERSON" | "SCOPE_ORG" | "SCOPE_GLOBAL";

export type AssertionType =
  | "ASSERTION_TYPE_UNSPECIFIED"
  | "ASSERT_USER_PREFERENCE"
  | "ASSERT_WORLD_FACT"
  | "ASSERT_DECISION"
  | "ASSERT_PROCEDURE"
  | "ASSERT_CONFLICT_EXPLANATION";

export type PrimitiveValue = string | boolean | number;

export interface ManifestHandle {
  ref: string;
  typeId: string;
  availability: string;
  subject?: string;
  predicateLabel?: string;
  signatureSummary?: string;
}

export interface ManifestBudget {
  maxOps: number;
  maxJoinDepth: number;
  maxFanout: number;
  maxTotalCost: number;
}

export interface Manifest {
  requestId: string;
  handles: ManifestHandle[];
  selectorRefs: string[];
  raw: unknown;
}

export type PlanOp =
  | { kind: "fetch"; handleRef: string }
  | { kind: "applySelector"; selectorRef: string; params?: Record<string, PrimitiveValue> }
  | { kind: "resolve"; inReg: string; policyId?: string }
  | { kind: "filter"; inReg: string; filterRef: string; params?: Record<string, PrimitiveValue> }
  | { kind: "join"; leftReg: string; rightReg: string; edgeType: string }
  | { kind: "project"; inReg: string; fieldPaths: string[] }
  | {
      kind: "assert";
      assertionType: AssertionType;
      bindings: Record<string, { reg: string; fieldPath: string }>;
      citations?: Array<{ handleRef?: string; anchorRef?: string }>;
    };

export interface PlanStep {
  out: string;
  op: PlanOp;
}

export interface PlanInput {
  requestId: string;
  steps: PlanStep[];
  outputs: string[];
}

export interface AppendEventInput {
  requestId: string;
  subject: string;
  text: string;
  scope?: Scope;
}

export interface AppendEventResult {
  eventId: string;
  handleRefs: string[];
}

export interface ExecutePlanInput {
  requestId: string;
  manifest: Manifest;
  plan: PlanInput;
}

export interface ExecutePlanResult {
  status: string;
  verifiedBlocks: string[];
  narrativeBlocks: string[];
  semanticRoot?: string;
  traceRoot?: string;
  stallHandleRef?: string;
  stallAvailability?: string;
}

export interface ForgetInput {
  requestId: string;
  subject: string;
  predicateLabel: string;
  scope?: Scope;
  reason?: string;
}

export interface ForgetResult {
  status: string;
  verifiedBlocks: string[];
}
