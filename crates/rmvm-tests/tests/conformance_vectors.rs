use rmvm_tests::conformance::{
    compare_expected, load_vectors, run_vector, validate_vector_conventions, vector_schema_path,
};

#[test]
fn schema_file_exists() {
    let schema = vector_schema_path();
    assert!(
        schema.exists(),
        "conformance schema is missing at {}",
        schema.display()
    );
}

#[test]
fn vectors_match_schema_conventions_and_expectations() {
    let vectors = load_vectors().expect("failed loading conformance vectors");
    assert!(
        !vectors.is_empty(),
        "expected conformance vectors, found none"
    );

    let mut failures = Vec::new();
    for (path, vector) in vectors {
        failures.extend(
            validate_vector_conventions(&path, &vector)
                .into_iter()
                .map(|e| format!("{}: {}", vector.vector_id, e)),
        );
        match run_vector(&vector) {
            Ok(result) => {
                failures.extend(
                    compare_expected(&vector, &result)
                        .into_iter()
                        .map(|e| format!("{}: {}", vector.vector_id, e)),
                );
            }
            Err(err) => failures.push(format!("{}: execution failed: {}", vector.vector_id, err)),
        }
    }
    assert!(
        failures.is_empty(),
        "conformance vector failures:\n{}",
        failures.join("\n")
    );
}
