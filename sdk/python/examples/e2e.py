from cortex_rmvm_sdk import (
    AssertBinding,
    AssertOp,
    CortexRmvmClient,
    FetchOp,
    PlanInput,
    PlanStep,
    ProjectOp,
    build_plan_only_prompt,
)


def main() -> None:
    client = CortexRmvmClient("127.0.0.1:50051")
    request_id = "py-e2e-001"
    subject = "user:vinz"
    user_message = "I prefer Earl Grey."

    client.append_event(
        request_id=request_id,
        subject=subject,
        text=user_message,
        scope="SCOPE_GLOBAL",
    )

    manifest = client.get_manifest(request_id)
    print("Plan-only prompt:")
    print(build_plan_only_prompt(user_message, manifest))

    if not manifest.handles:
        raise RuntimeError("manifest has no handles")
    handle_ref = manifest.handles[0].ref

    # Mocked planner output.
    plan = PlanInput(
        request_id=request_id,
        steps=[
            PlanStep(out="r0", op=FetchOp(handle_ref=handle_ref)),
            PlanStep(out="r1", op=ProjectOp(in_reg="r0", field_paths=["meta.subject"])),
            PlanStep(
                out="r2",
                op=AssertOp(
                    assertion_type="ASSERT_WORLD_FACT",
                    bindings={
                        "subject": AssertBinding(reg="r1", field_path="meta.subject"),
                    },
                ),
            ),
        ],
        outputs=["r2"],
    )

    result = client.execute_plan(request_id=request_id, manifest=manifest, plan=plan)
    print("Verified blocks:")
    for line in result.verified_blocks:
        print(f"- {line}")

    forget = client.forget(
        request_id=request_id,
        subject=subject,
        predicate_label="prefers_beverage",
        scope="SCOPE_GLOBAL",
        reason="suppress preference",
    )
    print("Forget confirmation:")
    for line in forget.verified_blocks:
        print(f"- {line}")

    client.close()


if __name__ == "__main__":
    main()
