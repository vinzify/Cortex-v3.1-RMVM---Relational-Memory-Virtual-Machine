use std::collections::BTreeSet;

use rmvm_tests::conformance::load_vectors;

#[test]
fn conformance_vector_catalog_has_mandatory_coverage() {
    let vectors = load_vectors().expect("failed loading conformance vectors");
    assert!(
        vectors.len() >= 25,
        "expected at least 25 conformance vectors, got {}",
        vectors.len()
    );

    let ids = vectors
        .into_iter()
        .map(|(_, v)| v.vector_id)
        .collect::<BTreeSet<_>>();
    let required = [
        "C31-SEC-001-selector-spoofing",
        "C31-SSA-003-register-smuggling-value-ref",
        "C31-COST-001-wide-select-cost-bypass",
        "C31-COST-003-join-depth-overflow",
        "C31-REF-001-unknown-handle-ref",
        "C31-REF-002-unknown-selector-ref",
        "C31-TYPE-001-selector-param-type-mismatch",
        "C31-SEC-003-broken-lineage-direct-citation",
        "C31-SEC-004-broken-lineage-transitive",
        "C31-STALL-001-offline-fetch-stall",
        "C31-STALL-002-archival-pending-fetch-stall",
        "C31-SEC-005-trust-gate-policy-tier2-required",
        "C31-SEC-006-taint-gate-web-untrusted-policy-sink",
        "C31-NARR-001-token-guard-unbound-number",
        "C31-DET-001-map-order-permutation-a",
        "C31-DET-002-map-order-permutation-b",
        "C31-DET-003-citation-order-permutation-a",
        "C31-DET-004-citation-order-permutation-b",
    ];

    let mut missing = Vec::new();
    for req in required {
        if !ids.contains(req) {
            missing.push(req.to_string());
        }
    }
    assert!(
        missing.is_empty(),
        "missing mandatory vectors: {}",
        missing.join(", ")
    );
}
