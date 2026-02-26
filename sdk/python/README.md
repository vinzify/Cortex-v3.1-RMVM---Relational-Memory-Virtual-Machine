# Python SDK

## Prerequisites
- Python 3.10+
- RMVM gRPC server running locally

Start server:

```bash
cargo run -p rmvm-grpc --bin rmvm-grpc-server
```

## Install

```bash
cd sdk/python
py -m pip install -e .
```

## Hello Memory (Mocked Plan)

```bash
py examples/e2e.py
```

What it does:
- appends a user preference event
- fetches manifest
- builds a plan-only prompt
- executes a mocked `RMVMPlan`
- prints verified blocks and forget confirmation

## STALL + Retry

```bash
py examples/stall_retry.py
```

What it does:
- appends an event and fetches manifest
- executes with mocked `OFFLINE` availability to force `STALL`
- flips availability to `READY`
- retries the same plan and prints final status
