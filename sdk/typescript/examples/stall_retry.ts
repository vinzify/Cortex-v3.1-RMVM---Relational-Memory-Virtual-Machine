import { CortexRmvmClient, type PlanInput } from "../src/index.js";

async function main(): Promise<void> {
  const client = new CortexRmvmClient("127.0.0.1:50051");
  const requestId = "ts-stall-001";
  const subject = "user:vinz";

  await client.appendEvent({
    requestId,
    subject,
    text: "I prefer oolong tea.",
    scope: "SCOPE_GLOBAL",
  });

  const manifest = await client.getManifest(requestId);
  const handleRef = manifest.handles[0]?.ref;
  if (!handleRef) throw new Error("manifest has no handles");

  const plan: PlanInput = {
    requestId,
    steps: [{ out: "r0", op: { kind: "fetch", handleRef } }],
    outputs: ["r0"],
  };

  // Force STALL by mocking handle availability in outbound manifest payload.
  const raw = manifest.raw as Record<string, unknown>;
  const handles = raw.handles as Array<Record<string, unknown>>;
  handles[0].availability = "OFFLINE";

  const stalled = await client.executePlan({ requestId, manifest, plan });
  console.log(`First execute status: ${stalled.status}`);
  console.log(`Stall handle: ${stalled.stallHandleRef} (${stalled.stallAvailability})`);

  // Retry once availability is restored.
  handles[0].availability = "READY";
  const retried = await client.executePlan({ requestId, manifest, plan });
  console.log(`Retry execute status: ${retried.status}`);

  client.close();
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
