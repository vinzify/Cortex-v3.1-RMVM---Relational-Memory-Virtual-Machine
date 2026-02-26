use rmvm_grpc::{GrpcKernelService, RmvmExecutorServer};
use tonic::transport::Server;
use std::env;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr_str = env::var("RMVM_SERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:50051".to_string());
    let addr = addr_str.parse()?;
    let max_decoding = env_usize("RMVM_MAX_DECODING_BYTES", 4 * 1024 * 1024);
    let max_encoding = env_usize("RMVM_MAX_ENCODING_BYTES", 4 * 1024 * 1024);
    let timeout_secs = env_u64("RMVM_REQUEST_TIMEOUT_SECS", 30);

    let service = GrpcKernelService::default();
    let service = RmvmExecutorServer::new(service)
        .max_decoding_message_size(max_decoding)
        .max_encoding_message_size(max_encoding);
    println!(
        "RMVM gRPC server listening on {} (decode={} encode={} timeout={}s)",
        addr, max_decoding, max_encoding, timeout_secs
    );
    Server::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .add_service(service)
        .serve(addr)
        .await?;
    Ok(())
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}
