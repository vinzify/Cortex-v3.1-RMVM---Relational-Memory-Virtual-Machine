use std::fs;
use std::path::PathBuf;

use prost::Message;
use rmvm_grpc::fixture_data::golden_execute_request;
use rmvm_kernel::{ExecuteOptions, execute};

fn main() {
    let req = golden_execute_request();
    let resp = execute(req.clone(), ExecuteOptions::default());

    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    fs::create_dir_all(&fixture_dir).expect("failed to create fixture directory");

    fs::write(
        fixture_dir.join("golden_execute_request.pb"),
        req.encode_to_vec(),
    )
    .expect("failed to write request fixture");
    fs::write(
        fixture_dir.join("golden_execute_response.pb"),
        resp.encode_to_vec(),
    )
    .expect("failed to write response fixture");
}
