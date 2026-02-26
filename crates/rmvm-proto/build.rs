fn main() {
    let protoc = protoc_bin_vendored::protoc_bin_path().expect("failed to resolve protoc");
    // SAFETY: Build scripts run in a dedicated process and setting an env var here only
    // affects this process and children during code generation.
    unsafe {
        std::env::set_var("PROTOC", protoc);
    }

    let mut cfg = prost_build::Config::new();
    cfg.btree_map(["."]);
    cfg.compile_protos(&["proto/cortex_rmvm_v3_1.proto"], &["proto"])
        .expect("failed to compile RMVM protobuf schema");
}
