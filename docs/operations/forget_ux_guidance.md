# Forget UX Guidance

## User-Facing Semantics
- `Forget` in v3.1 is **suppress**, not hard-delete.
- Suppress means:
  - preference/fact is removed from active manifest surfacing
  - execution returns verified confirmation output
  - historical evidence remains in append-only lineage unless a separate hard-delete workflow exists

## UX Copy Requirements
- Always describe action as:
  - "suppressed from active memory"
- Do not describe as:
  - "deleted forever"

## Audit Visibility
- Every successful suppress should display:
  - subject
  - predicate label
  - suppressed count
- Keep confirmation block visible in audit/history timeline.

## Degraded Lineage Mode Behavior
- Default mode:
  - broken-lineage handles cannot be asserted; requests reject with `ERROR_BROKEN_LINEAGE`.
- Degraded mode (`degraded_mode=true`):
  - allows controlled operation on broken lineage with warnings.
- UX requirement:
  - clearly label degraded mode output as "lineage degraded"
  - do not present degraded assertions as fully verified facts

## Support Playbook
- If user disputes memory persistence after forget:
  - verify suppress confirmation block in audit log
  - verify subsequent manifest excludes suppressed handle
  - escalate to compliance workflow only for hard-delete requests
