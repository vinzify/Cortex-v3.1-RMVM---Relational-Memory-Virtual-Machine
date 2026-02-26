# Security Review Checklist

## Metadata Sanitization (Trojan TOC)
- [ ] Validate sanitization of `signature_summary`, selector descriptions, predicate labels.
- [ ] Test bidi controls and homoglyph injection payloads.
- [ ] Ensure sanitized output cannot alter selector/handle routing.
- [ ] Enforce maximum metadata byte lengths and reject over-limit inputs.

## Narrative Token-Guard Fuzzing
- [ ] Build corpus of number/date/proper-noun injection attempts.
- [ ] Include boundary fuzzing around `{A[i].field}` and `{{macro.name}}` tokens.
- [ ] Assert invalid templates are rejected with `DATA_LEAK_PREVENTION`.
- [ ] Track rejection/acceptance counts and grammar branch coverage.

## Capability Sink Enforcement
- [ ] Verify taint propagation through fetch/select/join/project/assert.
- [ ] Verify policy sink rejects tainted (`TAINT_WEB_UNTRUSTED`, `TAINT_MIXED`) data.
- [ ] Verify trust + taint gate interactions are deterministic and error-coded.
- [ ] Confirm no silent taint downgrade in derived registers.

## gRPC Abuse Cases
- [ ] Oversized message tests:
  - request and response size limits enforced.
- [ ] Malformed protobuf payload tests:
  - invalid enums, invalid oneof payloads, missing required semantic combinations.
- [ ] Replay attempt tests:
  - repeated request IDs and duplicated forget/append behavior.
- [ ] Abuse-rate tests:
  - high fanout / deep plan attempts rejected via structured errors.

## Release Blocking Conditions
- [ ] All above test groups mapped to automated CI checks.
- [ ] Any open high severity finding has owner + remediation ETA.
- [ ] Determinism and conformance artifacts attached to release candidate.
