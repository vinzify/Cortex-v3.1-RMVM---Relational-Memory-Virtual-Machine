# Hello Memory (Python)

## 1. Start Server

```bash
cargo run -p rmvm-grpc --bin rmvm-grpc-server
```

## 2. Run Hello Memory

```bash
cd sdk/python
py -m pip install -e .
py examples/e2e.py
```

Expected output includes:
- plan-only prompt text
- verified blocks from `execute_plan`
- forget confirmation block

## 3. Run STALL + Retry

```bash
cd sdk/python
py examples/stall_retry.py
```

Expected output:
- first execution `STALL`
- second execution after retry `OK`
