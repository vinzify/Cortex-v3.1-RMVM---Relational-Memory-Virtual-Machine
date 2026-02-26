# Security Model (Portable Brain + Proxy UX)

## Threat model
- Brain exfiltration from filesystem/backups.
- Local key leakage from process/env/config.
- Prompt injection via user/metadata content.
- Replay attacks against proxy requests.
- Cross-tenant access via API key mix-ups.

## Controls in this implementation
- Encryption at rest for state and signing key (`XChaCha20-Poly1305`).
- Passphrase-derived key (`Argon2id`) from configured env var.
- Signed brain manifest (`Ed25519`) with verification on load/import.
- API key mapping by SHA-256 hash to `tenant_id + brain_id`.
- Least-privilege attachment records (`read/write/sinks`) stored and auditable.
- Signed/auditable operation trail in brain state (`audit` entries).

## Policy boundary
- Agent attachments define intended permissions:
  - read classes
  - write classes
  - sink allow-list
- Proxy enforces tenant->brain mapping before RMVM calls.
- RMVM kernel still enforces trust/taint/cost gates during `Execute`.

## Known v0 gaps
- Replay nonce cache is not yet persisted (planned next increment).
- Attachment policy is recorded and auditable; full runtime sink enforcement hooks are staged for next increment.
- Local secrets are env-driven; OS keyring integration is planned.
