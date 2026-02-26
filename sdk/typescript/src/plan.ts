import type { Manifest, PlanInput } from "./types.js";

export function buildPlanOnlyPrompt(userMessage: string, manifest: Manifest): string {
  const allowedHandles = manifest.handles.map((h) => h.ref).join(", ");
  const allowedSelectors = manifest.selectorRefs.join(", ");
  return [
    "Return plan JSON only. Do not include prose.",
    "Generate a valid RMVMPlan with requestId, steps, outputs.",
    `User message: ${userMessage}`,
    `Allowed handle refs: [${allowedHandles}]`,
    `Allowed selector refs: [${allowedSelectors}]`,
    "Every fetch.handleRef must be from allowed handle refs.",
    "Every applySelector.selectorRef must be from allowed selector refs.",
  ].join("\n");
}

export function validatePlanAgainstManifest(plan: PlanInput, manifest: Manifest): void {
  const handleRefs = new Set(manifest.handles.map((h) => h.ref));
  const selectorRefs = new Set(manifest.selectorRefs);
  const regs = new Set<string>();

  for (const step of plan.steps) {
    if (!step.out) {
      throw new Error("invalid plan: step.out is required");
    }
    if (regs.has(step.out)) {
      throw new Error(`invalid plan: register redefined (${step.out})`);
    }

    switch (step.op.kind) {
      case "fetch":
        if (!handleRefs.has(step.op.handleRef)) {
          throw new Error(`invalid plan: unknown handle ref ${step.op.handleRef}`);
        }
        break;
      case "applySelector":
        if (!selectorRefs.has(step.op.selectorRef)) {
          throw new Error(`invalid plan: unknown selector ref ${step.op.selectorRef}`);
        }
        break;
      case "resolve":
      case "filter":
      case "project":
        if (!regs.has(step.op.inReg)) {
          throw new Error(`invalid plan: input register not defined (${step.op.inReg})`);
        }
        break;
      case "join":
        if (!regs.has(step.op.leftReg) || !regs.has(step.op.rightReg)) {
          throw new Error("invalid plan: join registers not defined");
        }
        break;
      case "assert":
        for (const binding of Object.values(step.op.bindings)) {
          if (!regs.has(binding.reg)) {
            throw new Error(`invalid plan: assert binding register not defined (${binding.reg})`);
          }
        }
        break;
      default:
        break;
    }

    regs.add(step.out);
  }
}
