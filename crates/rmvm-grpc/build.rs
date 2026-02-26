fn main() {
    let protoc = protoc_bin_vendored::protoc_bin_path().expect("failed to resolve protoc");
    // SAFETY: build scripts are single-process build steps.
    unsafe {
        std::env::set_var("PROTOC", protoc);
    }

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .extern_path(
            ".cortex.rmvm.v3_1.ExecuteRequest",
            "::rmvm_proto::cortex::rmvm::v3_1::ExecuteRequest",
        )
        .extern_path(
            ".cortex.rmvm.v3_1.ExecuteResponse",
            "::rmvm_proto::cortex::rmvm::v3_1::ExecuteResponse",
        )
        .extern_path(
            ".cortex.rmvm.v3_1.PublicManifest",
            "::rmvm_proto::cortex::rmvm::v3_1::PublicManifest",
        )
        .extern_path(
            ".cortex.rmvm.v3_1.ExecutionStatus",
            "::rmvm_proto::cortex::rmvm::v3_1::ExecutionStatus",
        )
        .extern_path(
            ".cortex.rmvm.v3_1.VerifiedAssertion",
            "::rmvm_proto::cortex::rmvm::v3_1::VerifiedAssertion",
        )
        .extern_path(
            ".cortex.rmvm.v3_1.RenderedOutput",
            "::rmvm_proto::cortex::rmvm::v3_1::RenderedOutput",
        )
        .extern_path(
            ".cortex.rmvm.v3_1.ExecutionError",
            "::rmvm_proto::cortex::rmvm::v3_1::ExecutionError",
        )
        .extern_path(
            ".cortex.rmvm.v3_1.Scope",
            "::rmvm_proto::cortex::rmvm::v3_1::Scope",
        )
        .compile_protos(
            &["../../proto/cortex_rmvm_v3_1_service.proto"],
            &["../../proto"],
        )
        .expect("failed to compile gRPC service proto");
}
