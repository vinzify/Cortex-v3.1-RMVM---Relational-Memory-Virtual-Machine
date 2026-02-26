# Forget Semantics (Outline)

## Definitions
- `suppress`:
  - logical hide/block from future manifest selection and policy usage
  - produces auditable verified confirmation output
- `hard delete`:
  - irreversible physical data removal path
  - separate compliance-governed workflow

## Current v3.1 Behavior
- `Forget` implements `suppress` semantics.
- Expected output:
  - `ExecutionStatus=OK`
  - verified confirmation block with suppressed count/subject/predicate

## Audit and Provenance
- Suppress action must remain traceable via response assertions and citations.
- Suppress does not rewrite historical event ledger.

## Operator Guidance
- Use `suppress` for user preference and policy exclusion workflows.
- Reserve `hard delete` for legal/compliance deletion requests only.

## Future Hard Delete Section (vNext)
- define required authorization model
- define irreversible tombstone semantics
- define compliance evidence artifacts
