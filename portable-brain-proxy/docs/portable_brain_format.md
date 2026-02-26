# Portable Brain Format (v1)

A brain is a versioned workspace persisted under `~/.cortex/brains/<brain-id>`.

## Files
- `brain.json`
  - format/version metadata
  - tenant + brain identifiers
  - migration markers
  - KDF salt
  - signing public key
  - encrypted state checksum
  - signature over canonical manifest payload
- `state.enc`
  - encrypted JSON payload containing:
    - branch states
    - memory objects
    - rules
    - suppressions
    - attachments
    - audit log
- `keys/signing_key.enc`
  - encrypted Ed25519 private signing key

## Export (`.cbrain`)
Single JSON package with:
- manifest
- encrypted state blob
- encrypted signing key blob

## Crypto
- KDF: Argon2id
- Encryption: XChaCha20-Poly1305
- Signing: Ed25519

## Migrations
- `brain/v1:init` initial schema marker
- `schema_migrations` supports additive migration tracking
