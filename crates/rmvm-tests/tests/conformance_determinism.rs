use rmvm_tests::conformance::{compare_expected, load_baseline_hash, load_vectors, run_vector};

#[test]
fn vectors_match_baseline_bytes_and_repeated_runs() {
    let vectors = load_vectors().expect("failed loading vectors");
    let mut failures = Vec::new();

    for (_, vector) in vectors {
        let first = match run_vector(&vector) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!("{}: first execution failed: {}", vector.vector_id, e));
                continue;
            }
        };
        let second = match run_vector(&vector) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!("{}: second execution failed: {}", vector.vector_id, e));
                continue;
            }
        };

        failures.extend(
            compare_expected(&vector, &first)
                .into_iter()
                .map(|e| format!("{}: {}", vector.vector_id, e)),
        );

        if first.response_sha256 != second.response_sha256 {
            failures.push(format!(
                "{}: repeated run hash mismatch: {} vs {}",
                vector.vector_id, first.response_sha256, second.response_sha256
            ));
        }

        match load_baseline_hash(&vector.vector_id) {
            Some(expected) => {
                if expected != first.response_sha256 {
                    failures.push(format!(
                        "{}: baseline hash mismatch: expected {}, got {}",
                        vector.vector_id, expected, first.response_sha256
                    ));
                }
            }
            None => failures.push(format!("{}: baseline is missing", vector.vector_id)),
        }
    }

    assert!(
        failures.is_empty(),
        "determinism failures:\n{}",
        failures.join("\n")
    );
}
