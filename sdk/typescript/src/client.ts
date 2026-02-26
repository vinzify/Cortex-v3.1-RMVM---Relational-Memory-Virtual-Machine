import path from "node:path";
import { fileURLToPath } from "node:url";

import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";

import { validatePlanAgainstManifest } from "./plan.js";
import type {
  AppendEventInput,
  AppendEventResult,
  ExecutePlanInput,
  ExecutePlanResult,
  ForgetInput,
  ForgetResult,
  Manifest,
  PlanInput,
  PrimitiveValue,
  Scope,
} from "./types.js";

type GrpcClient = grpc.Client & Record<string, (...args: unknown[]) => void>;

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PROTO_DIR = path.resolve(__dirname, "../proto");
const SERVICE_PROTO = path.join(PROTO_DIR, "cortex_rmvm_v3_1_service.proto");

const packageDef = protoLoader.loadSync(SERVICE_PROTO, {
  keepCase: true,
  longs: String,
  enums: String,
  defaults: true,
  oneofs: true,
  includeDirs: [PROTO_DIR],
});
const loaded = grpc.loadPackageDefinition(packageDef) as Record<string, unknown>;
const ClientCtor = (((loaded.cortex as Record<string, unknown>).rmvm as Record<string, unknown>)
  .v3_1 as Record<string, unknown>).RmvmExecutor as grpc.ServiceClientConstructor;

const scopeToProto: Record<Scope, string> = {
  SCOPE_UNSPECIFIED: "SCOPE_UNSPECIFIED",
  SCOPE_SESSION: "SCOPE_SESSION",
  SCOPE_PROJECT: "SCOPE_PROJECT",
  SCOPE_PERSON: "SCOPE_PERSON",
  SCOPE_ORG: "SCOPE_ORG",
  SCOPE_GLOBAL: "SCOPE_GLOBAL",
};

const assertionTypeToProto: Record<string, string> = {
  ASSERTION_TYPE_UNSPECIFIED: "ASSERTION_TYPE_UNSPECIFIED",
  ASSERT_USER_PREFERENCE: "ASSERT_USER_PREFERENCE",
  ASSERT_WORLD_FACT: "ASSERT_WORLD_FACT",
  ASSERT_DECISION: "ASSERT_DECISION",
  ASSERT_PROCEDURE: "ASSERT_PROCEDURE",
  ASSERT_CONFLICT_EXPLANATION: "ASSERT_CONFLICT_EXPLANATION",
};

export class CortexRmvmClient {
  private readonly client: GrpcClient;

  constructor(address: string, credentials?: grpc.ChannelCredentials) {
    this.client = new ClientCtor(
      address,
      credentials ?? grpc.credentials.createInsecure()
    ) as GrpcClient;
  }

  async appendEvent(input: AppendEventInput): Promise<AppendEventResult> {
    const resp = await this.unaryCall("AppendEvent", {
      request_id: input.requestId,
      subject: input.subject,
      text: input.text,
      scope: scopeToProto[input.scope ?? "SCOPE_GLOBAL"],
    });
    return {
      eventId: resp.event_id as string,
      handleRefs: (resp.handle_refs as string[]) ?? [],
    };
  }

  async getManifest(requestId: string): Promise<Manifest> {
    const resp = await this.unaryCall("GetManifest", { request_id: requestId });
    const rawManifest = resp.manifest as Record<string, unknown>;
    const handles = ((rawManifest.handles as Record<string, unknown>[]) ?? []).map((h) => ({
      ref: String(h.ref ?? ""),
      typeId: String(h.type_id ?? ""),
      availability: String(h.availability ?? "HANDLE_AVAILABILITY_UNSPECIFIED"),
      subject: (h.meta as Record<string, unknown> | undefined)?.subject as string | undefined,
      predicateLabel: (h.meta as Record<string, unknown> | undefined)
        ?.predicate_label as string | undefined,
      signatureSummary: (h.signature_summary as string | undefined) ?? undefined,
    }));
    const selectorRefs = ((rawManifest.selectors as Record<string, unknown>[]) ?? []).map((s) =>
      String(s.sel ?? "")
    );
    return {
      requestId: String(rawManifest.request_id ?? requestId),
      handles,
      selectorRefs,
      raw: rawManifest,
    };
  }

  async executePlan(input: ExecutePlanInput): Promise<ExecutePlanResult> {
    validatePlanAgainstManifest(input.plan, input.manifest);
    const protoPlan = planToProto(input.plan);
    const resp = await this.unaryCall("Execute", {
      manifest: input.manifest.raw,
      plan: protoPlan,
    });
    return {
      status: String(resp.status ?? "EXECUTION_STATUS_UNSPECIFIED"),
      verifiedBlocks:
        ((resp.rendered as Record<string, unknown> | undefined)?.verified_blocks as string[]) ??
        [],
      narrativeBlocks:
        ((resp.rendered as Record<string, unknown> | undefined)?.narrative_blocks as string[]) ??
        [],
      semanticRoot: (resp.proof as Record<string, unknown> | undefined)?.semantic_root as
        | string
        | undefined,
      traceRoot: (resp.proof as Record<string, unknown> | undefined)?.trace_root as
        | string
        | undefined,
      stallHandleRef: (resp.stall as Record<string, unknown> | undefined)?.handle_ref as
        | string
        | undefined,
      stallAvailability: (resp.stall as Record<string, unknown> | undefined)?.availability as
        | string
        | undefined,
    };
  }

  async forget(input: ForgetInput): Promise<ForgetResult> {
    const resp = await this.unaryCall("Forget", {
      request_id: input.requestId,
      subject: input.subject,
      predicate_label: input.predicateLabel,
      scope: scopeToProto[input.scope ?? "SCOPE_GLOBAL"],
      reason: input.reason ?? "",
    });
    return {
      status: String(resp.status ?? "EXECUTION_STATUS_UNSPECIFIED"),
      verifiedBlocks:
        ((resp.rendered as Record<string, unknown> | undefined)?.verified_blocks as string[]) ??
        [],
    };
  }

  close(): void {
    this.client.close();
  }

  private unaryCall(method: string, payload: Record<string, unknown>): Promise<Record<string, unknown>> {
    return new Promise((resolve, reject) => {
      (this.client[method] as (req: Record<string, unknown>, cb: (err: grpc.ServiceError | null, resp: Record<string, unknown>) => void) => void)(
        payload,
        (err, resp) => {
          if (err) {
            reject(err);
            return;
          }
          resolve(resp);
        }
      );
    });
  }
}

function planToProto(plan: PlanInput): Record<string, unknown> {
  return {
    request_id: plan.requestId,
    steps: plan.steps.map((step) => ({
      out: step.out,
      ...opToProto(step.op),
    })),
    outputs: plan.outputs.map((reg) => ({ reg })),
  };
}

function opToProto(op: PlanInput["steps"][number]["op"]): Record<string, unknown> {
  switch (op.kind) {
    case "fetch":
      return { fetch: { handle_ref: op.handleRef } };
    case "applySelector":
      return {
        apply_selector: {
          selector_ref: op.selectorRef,
          params: mapValueMap(op.params ?? {}),
        },
      };
    case "resolve":
      return { resolve: { in_reg: op.inReg, policy_id: op.policyId ?? "" } };
    case "filter":
      return {
        filter: {
          in_reg: op.inReg,
          filter_ref: op.filterRef,
          params: mapValueMap(op.params ?? {}),
        },
      };
    case "join":
      return {
        join: {
          left_reg: op.leftReg,
          right_reg: op.rightReg,
          edge_type: op.edgeType,
        },
      };
    case "project":
      return { project: { in_reg: op.inReg, field_paths: op.fieldPaths } };
    case "assert":
      return {
        assert_op: {
          assertion_type: assertionTypeToProto[op.assertionType],
          bindings: Object.fromEntries(
            Object.entries(op.bindings).map(([k, v]) => [k, { reg: v.reg, field_path: v.fieldPath }])
          ),
          citations: (op.citations ?? []).map((c) =>
            c.handleRef ? { handle_ref: c.handleRef } : { anchor_ref: c.anchorRef ?? "" }
          ),
        },
      };
    default:
      return {};
  }
}

function mapValueMap(values: Record<string, PrimitiveValue>): Record<string, Record<string, PrimitiveValue>> {
  return Object.fromEntries(Object.entries(values).map(([k, v]) => [k, primitiveToProtoValue(v)]));
}

function primitiveToProtoValue(value: PrimitiveValue): Record<string, PrimitiveValue> {
  if (typeof value === "string") {
    return { s: value };
  }
  if (typeof value === "boolean") {
    return { b: value };
  }
  if (Number.isInteger(value)) {
    return { i64: value };
  }
  return { f64: value };
}
