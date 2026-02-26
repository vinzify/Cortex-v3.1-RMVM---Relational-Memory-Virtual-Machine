# Security Guarantees (Outline)

## Guaranteed
- Referential integrity:
  - plan references are limited to manifest-issued handle/selector refs.
- Deterministic execution:
  - bounded op count, join depth, and cost guard enforcement.
- Availability safety:
  - non-ready handles return `STALL` with structured stall info.
- Trust gate:
  - policy-impacting assertions require trust tier `>= TIER_2`.
- Taint gate:
  - web-untrusted/mixed taint cannot sink into policy-impacting assertions.
- Lineage invalidation:
  - broken-lineage handles reject assertion surfacing in normal mode.
- Output guard:
  - narrative token guard blocks unbound factual tokens.
- Proof determinism:
  - stable semantic roots and deterministic CPE response bytes for fixed inputs.

## Not Guaranteed
- External source truthfulness beyond captured evidence.
- Human policy correctness in authored rule sets.
- Availability SLAs for archival/offline backends.
- Protection outside documented threat model and capability boundary.

## Verification Mechanism
- Conformance vectors + cross-OS determinism CI.
- Baseline CPE byte comparison and semantic root parity checks.
