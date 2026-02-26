import { CortexRmvmClient, buildPlanOnlyPrompt, type PlanInput } from "../src/index.js";

async function main(): Promise<void> {
  const client = new CortexRmvmClient("127.0.0.1:50051");
  const requestId = "ts-e2e-001";
  const subject = "user:vinz";
  const userMessage = "I prefer Earl Grey.";

  await client.appendEvent({
    requestId,
    subject,
    text: userMessage,
    scope: "SCOPE_GLOBAL",
  });

  const manifest = await client.getManifest(requestId);
  const planPrompt = buildPlanOnlyPrompt(userMessage, manifest);
  console.log("Plan-only prompt:\n", planPrompt);

  const handleRef = manifest.handles[0]?.ref;
  if (!handleRef) {
    throw new Error("manifest has no handles");
  }

  // Mocked planner output.
  const plan: PlanInput = {
    requestId,
    steps: [
      { out: "r0", op: { kind: "fetch", handleRef } },
      { out: "r1", op: { kind: "project", inReg: "r0", fieldPaths: ["meta.subject"] } },
      {
        out: "r2",
        op: {
          kind: "assert",
          assertionType: "ASSERT_WORLD_FACT",
          bindings: {
            subject: { reg: "r1", fieldPath: "meta.subject" },
          },
        },
      },
    ],
    outputs: ["r2"],
  };

  const exec = await client.executePlan({ requestId, manifest, plan });
  console.log("Verified blocks:");
  for (const line of exec.verifiedBlocks) {
    console.log(`- ${line}`);
  }

  const forget = await client.forget({
    requestId,
    subject,
    predicateLabel: "prefers_beverage",
    scope: "SCOPE_GLOBAL",
    reason: "suppress preference",
  });
  console.log("Forget confirmation:");
  for (const line of forget.verifiedBlocks) {
    console.log(`- ${line}`);
  }

  client.close();
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
