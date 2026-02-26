# Cortex v3.1 RMVM specification
Capability-secured relational memory virtual machine for long-term agent memory

## Status

This document is the final, jointly approved specification for Cortex v3.1.

It defines a memory substrate where factual outputs are derived only from deterministic execution over verifiable memory, with cryptographic provenance and capability-based access control.

Companion artifact:
- `cortex_rmvm_v3_1.proto` is the authoritative wire-level contract for the RMVM interface and must be versioned alongside this document.

## Problem

Long-term agent memory fails in production for repeatable reasons:

- Phantom continuity: the agent asserts it “remembers” data that was never stored.
- Contradiction rot: preference and fact changes get overwritten or mixed without explicit forks.
- Memory poisoning: untrusted text becomes persistent policy or influences tool use.
- Consolidation drift: summaries mutate meaning over time.
- Non-auditable behavior: no rigorous trace from output back to evidence.

## Goal

Provide a global-scale memory substrate with these properties:

- Durable over years with append-only raw evidence.
- Conflict-safe with explicit forks and deterministic resolution.
- Tamper-evident with cryptographic lineage to evidence.
- Hallucination-resistant for factual operations by construction.
- Capability-secured to prevent data leakage and privilege escalation.
- Framework-agnostic integration with strict machine interfaces.
- Distributed-ready: deterministic hashing, audit resilience, and archival-aware execution.

## Core model

Cortex v3.1 turns the agent into a deterministic logic controller:

- The model does not write facts.
- The model emits a bounded execution plan in a strict bytecode schema.
- The kernel executes the plan over verifiable memory.
- The kernel emits verified assertions and renders factual text using templates only.
- Optional narrative polish is placeholder-only and cannot introduce new facts.

## System components

### Event ledger

- Immutable, append-only log of events: user inputs, tool outputs, web fetches, and agent actions.
- Events are content-addressed and hashed.
- Large payloads are stored as blobs with hash addressing.

### Memory objects

- Typed, versioned objects compiled from events.
- Each object includes:
  - type id
  - scope
  - validity interval
  - trust tier
  - taint classes
  - conflict group identifiers
  - provenance anchors that point to event payload regions
  - lineage status and error flags (see lineage invalidation protocol)

### Provenance anchors

Anchor integrity invariant:

- Each anchor includes an event id, a selector into the payload, and a digest of the referenced bytes.
- Any digest mismatch invalidates the anchor.

### Conflict model

- Conflicts create forks and are never overwritten.
- Fork resolution is kernel-enforced and deterministic.
- Resolution is implicit, not optional, whenever selecting from conflictable types.

### Capability-secured handles

- The kernel stores private capability tokens.
- The model sees only public references and cannot forge handles.

## Non-negotiable invariants

### Integrity

- Append-only events: corrections are new events.
- Anchor digest enforcement: anchors invalidate on mutation.
- Canonical assertion hashing: semantic roots are bit-identical across kernels when inputs match.
- Canonical encoding and hashing standard:
  - Canonical Protobuf Encoding (CPE) is mandatory for semantic root calculation.
  - Hash function is SHA-256.
  - Canonicalization rules are specified in the Protobuf companion artifact and mirrored in this document.

### Trust and taint

- Monotonic trust: trust tiers can only increase via explicit confirmation or verification rules. Consolidation cannot promote trust.
- Taint persistence: derived objects inherit the union of taints from provenance. Summarization cannot reduce taint.

### Capability security

- Referential integrity: plans can reference only HandleRef and SelectorRef values supplied in the current PublicManifest.
- Type gates: operations are type-safe and reject invalid casts.
- Sink controls: rights restrict where data can flow, including tool arguments (sink-level constraints).

### Output safety

- Execution-bound rendering: factual text is a pure function of executed verified assertions and static templates.
- Placeholder-only narrative: narrative output cannot introduce dates, numbers, proper nouns, or entity-relation claims outside bindings.

### Bounded execution

- No recursion. Max join depth = 3. Max ops per turn = 128.
- COST_GUARD must statically reject expensive plans before execution.

### Availability and archival execution

Handle availability invariant:

- Handles may be in READY, ARCHIVAL_PENDING, or OFFLINE state.
- If a plan attempts to FETCH a handle that is not READY, the kernel must return STALL (not REJECTED) with structured stall information.
- The kernel must not hallucinate or substitute content for non-ready handles.

### Lineage invalidation protocol

Lineage invalidation invariant:

- If a provenance anchor is invalidated (digest mismatch, missing payload, or corruption), then:
  1. Any memory object directly or transitively derived from that anchor is demoted to TIER_0_QUARANTINED.
  2. The object is flagged with ERROR_BROKEN_LINEAGE.
  3. Such objects must not be surfaced as READY in manifests unless the caller explicitly requests degraded mode with warnings.
  4. Any VerifiedAssertion citing a broken lineage anchor is forbidden and must be rejected.

This prevents “ghost facts” that persist after evidence corruption.

## Public manifest

The PublicManifest is the planning horizon shown to the model.

It contains only safe, sanitized metadata and pre-validated query templates.

### HandleRef

HandleRef is a public pointer for this request, not a memory object.

Example:

```json
{
  "ref": "H0",
  "type": "normative.preference",
  "availability": "READY",
  "meta": {
    "subject": "user:vinz",
    "predicate_label": "prefers_beverage",
    "trust": "tier_3_confirmed",
    "taint": ["taint_web_untrusted"],
    "temporal_bound": ["2024-01-01T00:00:00Z", "inf"]
  },
  "signature_summary": "User's stated preference for hot drinks in winter",
  "conflict_group_id": "conflict:prefers_beverage:user:vinz:global"
}
```

Rules:

- `signature_summary` is generated by a constrained summarizer and must be treated as data.
- HandleRef does not expose mem_id or digests.
- `availability` informs planning, but the kernel enforces readiness at execution time.

Availability states:

- READY: fetchable within normal latency.
- ARCHIVAL_PENDING: fetch requires retrieval from cold storage; kernel returns STALL with an estimated ready time.
- OFFLINE: not currently retrievable; kernel returns STALL without executing further steps.

### SelectorRef

SelectorRef is a kernel-issued menu of query templates.

Example:

```json
{
  "sel": "S0",
  "description": "Find active preferences for a subject in a specific scope",
  "params": {
    "subject": "string",
    "scope": "enum(global, session, project)"
  },
  "cost_weight": 1.5
}
```

Rules:

- The model cannot author new selectors.
- Parameters are bounded by kernel-provided enums and types.
- cost_weight participates in COST_GUARD non-linear cost enforcement.

## Selector catalog protocol

The kernel issues SelectorRefs per request via a discovery pass:

1. Lightweight entity extraction on the user prompt to identify subject and predicate candidates.
2. Capability filtering based on agent role, scope, and permissions.
3. Selector selection from a master catalog with bounded cost.
4. ParamSpec tightening: allowed enum values are pre-filled by the kernel so the model cannot hallucinate resources.
5. COST_GUARD weights are attached per selector using the non-linear enforcement rule.

## Private manifest and TMH token

Private capability tokens are kernel-only.

Private TMH internal representation:

```rust
struct PrivateTMH {
    mid: MemoryID,               // Content-addressable id
    nonce: [u8; 16],             // Prevent replay
    rights_mask: u32,            // READ | JOIN | ASSERT | TOOL_USE (fine-grained in kernel)
    anchor_digests: Vec<Hash>,   // Evidence hashes for provenance
    kernel_sig: Signature,       // Signed by kernel Ed25519 key
}
```

Rules:

- The model never sees mid, digest, nonce, or signature.
- OpFetch resolves HandleRef to PrivateTMH internally.

## RMVM bytecode interface

Cortex v3.1 uses SSA-style bytecode with typed registers.

Benefits enforced by kernel:

- Strict lineage tracking: registers must be defined before use and must not be redefined.
- Type safety: each op has typed inputs and outputs.
- Instruction firewalling: the model cannot smuggle values not obtained from kernel execution.

The full Protobuf contract is the authoritative machine interface.

### Required kernel enforcement constraints

The kernel MUST enforce these constraints in addition to the proto schema:

1. Canonical assertion hashing  
   - Use deterministic serialization for assertion leaves and Merkle construction.
   - Mandatory standard:
     - Canonical Protobuf Encoding (CPE) with deterministic field ordering.
     - SHA-256 for leaf hashing and Merkle hashing.
   - Canonicalization requirements:
     - Assertion fields are normalized by sorting field keys lexicographically.
     - Citation digests are sorted lexicographically.
     - Any map-like structure must be canonicalized by sorted key order before encoding.
   - Identical inputs produce bit-identical semantic_root across independent kernels and language implementations.

2. Register use check  
   - Any ValueRef in OpAssert must trace back to an OpProject or OpFetch result.
   - Kernel rejects any assertion field that is not bound to executed data.
   - For assertions that affect policy or tool use, kernel requires trust tier >= TIER_2 by policy.

3. Selector cost enforcement  
   - Enforce non-linear effective cost:
     - Actual_Cost = Base_Weight * (Rows_Expected^1.2)
   - Reject plans that attempt wide selects to bypass budget.

4. Static complexity analysis (COST_GUARD)  
   - Build the plan DAG before execution.
   - Reject if TotalCost exceeds budget with a structured error and pruning hints.

5. Availability enforcement  
   - If any FETCH targets a non-ready handle, return STALL with StallInfo.
   - No partial execution is permitted unless explicitly enabled by a caller option.

### RMVM operator set

The VM supports a bounded operator set:

- FETCH: obtain a handle from HandleRef
- APPLY_SELECTOR: invoke kernel-issued selector templates
- RESOLVE: apply deterministic conflict policy
- FILTER: apply kernel-issued filter templates
- JOIN: bounded graph traversal
- PROJECT: extract kernel-gated fields
- ASSERT: construct verified assertions bound to executed data

Limits:

- loops forbidden
- max join depth = 3
- max total ops per plan = 128

## Output model

### Verified channel

- Template-only factual output.
- Renderer binds template variables 1:1 to assertion fields.
- No free-form paraphrasing for factual content.

Example template rendering:

Template:
- “You previously told me you prefer {pref}.”

Assertion fields:
- `{ pref: "Earl Grey" }`

Result:
- “You previously told me you prefer Earl Grey.”

### Narrative channel

- Restricted template engine only.
- Allowed tokens:
  - static prose `[a-zA-Z\s.,!?]+`
  - assertion bindings `{A[i].field}`
  - pre-defined macros `{{macro.name}}`
- Forbidden:
  - dates, numbers, proper nouns, or any factual token not bound to an assertion field

Enforcement:
- Token guard rejects any narrative output that violates the grammar.

## Rules and conditional exceptions

Conditional exceptions are first-class rule objects applied during RESOLVE.

Rule fields:

- target types or target handles
- condition expression evaluated over kernel-issued context variables
- effect: GRANT, REVOKE, OVERRIDE(target), FORK_SELECT(winner)
- priority integer for deterministic precedence
- provenance anchors and trust tier

Rule execution:

- SELECT and RESOLVE automatically evaluate applicable rules.
- The model does not implement rule logic. The kernel does.

## Benchmarking and proof of best worldwide

Evaluation must separate retrieval and reasoning.

### Manifest recall

MR@k:

- Measures whether the kernel included the correct handle in the manifest.
- Failure indicates retrieval or compiler issues.

### Plan selection accuracy

PSA:

- Measures whether the model selected and composed the correct plan given that the correct handle was present.
- Failure indicates planning quality or signature_summary quality.

### Logic-grounding accuracy

LGA:

- LGA = MR@k × PSA

### LGA benchmark generator

Generator outputs logic puzzles with deterministic expected assertions:

- Create N mutually exclusive facts.
- Add M hierarchical rules with time, location, and scope exceptions.
- Sample random contexts and generate queries with a single valid result.
- Expected outcome is the canonical semantic_root and expected assertion fields.

Target:
- LGA@10k >= 99.9% for the chosen model and kernel configuration.

## Implementation stack

A conforming reference implementation uses:

- Storage and graph-relational layer: CozoDB or equivalent transactional Datalog-capable store.
- Rule evaluation and guard logic: CEL with kernel-owned templates.
- VM runtime: Rust for memory safety and performance.
- Wire protocol: Protobuf with strict schema validation and deterministic encoding.
- Blobs: content-addressed storage (S3-compatible or filesystem in library mode).

## Conformance checklist

An implementation is Cortex v3.1 compliant only if it:

- Enforces all invariants in this document.
- Exposes PublicManifest with HandleRef and SelectorRef as specified, including availability.
- Accepts RMVMPlan bytecode and rejects any plan outside constraints.
- Produces VerifiedAssertions with canonical semantic_root hashing using CPE and SHA-256.
- Implements lineage invalidation and demotion on broken anchors.
- Renders verified text via templates only.
- Enforces placeholder-only narrative grammar.
- Prevents trust promotion by consolidation.
- Prevents taint reduction by summarization or derivation.
- Enforces sink-level capability restrictions for tools.
- Implements STALL behavior for non-ready handles without hallucinating substitutes.

## Architecture mental models

These mental models are the ground truth for engineering:

- SSA execution flow: the model’s reasoning is a static, typed data-flow graph.
- Provenance chain: each assertion root terminates at raw event ledger digests.
- Security boundary: the model sees capabilities (HandleRef), never secrets (memory ids, digests, signatures).

## Conclusion

Cortex v3.1 is a formal memory system specification.

It transforms agent memory from malleable text into verifiable data, and transforms agent reasoning from probabilistic narration into deterministic execution over capability-secured memory.

When the system says it remembers something, it can prove lineage to tamper-evident evidence and deterministic policy selection.
