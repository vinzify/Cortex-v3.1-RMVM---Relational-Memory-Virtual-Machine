from cortex_rmvm_sdk import CortexRmvmClient, FetchOp, PlanInput, PlanStep
from cortex_rmvm_sdk.generated import cortex_rmvm_v3_1_pb2 as pb2


def main() -> None:
    client = CortexRmvmClient("127.0.0.1:50051")
    request_id = "py-stall-001"
    subject = "user:vinz"

    client.append_event(
        request_id=request_id,
        subject=subject,
        text="I prefer jasmine tea.",
        scope="SCOPE_GLOBAL",
    )
    manifest = client.get_manifest(request_id)
    if not manifest.handles:
        raise RuntimeError("manifest has no handles")

    plan = PlanInput(
        request_id=request_id,
        steps=[PlanStep(out="r0", op=FetchOp(handle_ref=manifest.handles[0].ref))],
        outputs=["r0"],
    )

    # Force STALL by mocking non-ready availability in request manifest.
    manifest.raw.handles[0].availability = pb2.OFFLINE
    stalled = client.execute_plan(request_id=request_id, manifest=manifest, plan=plan)
    print(f"First execute status: {stalled.status}")
    print(f"Stall handle: {stalled.stall_handle_ref} ({stalled.stall_availability})")

    # Retry after setting handle to READY.
    manifest.raw.handles[0].availability = pb2.READY
    retried = client.execute_plan(request_id=request_id, manifest=manifest, plan=plan)
    print(f"Retry execute status: {retried.status}")

    client.close()


if __name__ == "__main__":
    main()
