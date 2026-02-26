# rmvm-grpc

gRPC transport layer for Cortex RMVM v3.1.

## Binary

Run server:

```bash
cargo run -p rmvm-grpc --bin rmvm-grpc-server
```

Environment variables:
- `RMVM_SERVER_ADDR` (default `127.0.0.1:50051`)
- `RMVM_MAX_DECODING_BYTES` (default `4194304`)
- `RMVM_MAX_ENCODING_BYTES` (default `4194304`)
- `RMVM_REQUEST_TIMEOUT_SECS` (default `30`)
