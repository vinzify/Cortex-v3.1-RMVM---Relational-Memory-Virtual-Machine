use std::fs;
use std::path::PathBuf;

use prost::Message;
use rmvm_grpc::{GrpcKernelService, RmvmExecutorClient, RmvmExecutorServer};
use rmvm_proto::{ExecuteRequest, ExecuteResponse, ExecutionStatus};
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;

fn fixture_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(file)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_execute_matches_golden_bytes_and_roots() {
    let req_bytes =
        fs::read(fixture_path("golden_execute_request.pb")).expect("missing request fixture");
    let expected_resp_bytes =
        fs::read(fixture_path("golden_execute_response.pb")).expect("missing response fixture");

    let request = ExecuteRequest::decode(req_bytes.as_slice()).expect("invalid request fixture");
    let expected_response =
        ExecuteResponse::decode(expected_resp_bytes.as_slice()).expect("invalid response fixture");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind failed");
    let addr = listener.local_addr().expect("local addr");
    let incoming = TcpListenerStream::new(listener);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let server = tokio::spawn(async move {
        Server::builder()
            .add_service(RmvmExecutorServer::new(GrpcKernelService::default()))
            .serve_with_incoming_shutdown(incoming, async move {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("grpc server failed");
    });

    let endpoint = format!("http://{addr}");
    let mut client = RmvmExecutorClient::connect(endpoint)
        .await
        .expect("client connect failed");
    let actual_response = client
        .execute(request)
        .await
        .expect("execute RPC failed")
        .into_inner();

    let actual_resp_bytes = actual_response.encode_to_vec();
    assert_eq!(
        actual_resp_bytes, expected_resp_bytes,
        "gRPC response bytes diverged from golden fixture"
    );
    assert_eq!(
        actual_response.status,
        ExecutionStatus::Ok as i32,
        "expected deterministic OK response"
    );

    let actual_proof = actual_response.proof.expect("missing proof");
    let expected_proof = expected_response.proof.expect("missing expected proof");
    assert_eq!(actual_proof.semantic_root, expected_proof.semantic_root);
    assert_eq!(actual_proof.trace_root, expected_proof.trace_root);

    let _ = shutdown_tx.send(());
    server.await.expect("server task join failed");
}
