use std::env;
use std::process::ExitCode;

use rmvm_tests::conformance::{
    compare_expected, conformance_root, load_baseline_bytes, load_baseline_hash, load_vectors,
    run_vector, update_vector_expectations, vector_report, write_baseline, write_drift_artifacts,
    write_report,
};

fn main() -> ExitCode {
    let mode = env::args().nth(1).unwrap_or_else(|| "check".to_string());
    let run_id = env::var("RMVM_RUN_ID").unwrap_or_else(|_| "local".to_string());
    let os_id = env::var("RMVM_OS_ID").unwrap_or_else(|_| env::consts::OS.to_string());

    let vectors = match load_vectors() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("failed loading vectors: {e}");
            return ExitCode::from(1);
        }
    };

    let root = conformance_root();
    let mut all_failures = Vec::new();
    for (path, vector) in vectors {
        let result = match run_vector(&vector) {
            Ok(r) => r,
            Err(e) => {
                all_failures.push(format!("{}: execution failed: {}", vector.vector_id, e));
                continue;
            }
        };

        if mode == "sync" {
            if let Err(e) = write_baseline(&vector, &result) {
                all_failures.push(format!("{}: baseline write failed: {}", vector.vector_id, e));
            }
            if let Err(e) = update_vector_expectations(&path, &vector, &result) {
                all_failures.push(format!(
                    "{}: vector expectation sync failed: {}",
                    vector.vector_id, e
                ));
            }
        }

        let mut failures = if mode == "sync" {
            Vec::new()
        } else {
            compare_expected(&vector, &result)
        };
        if mode != "sync" {
            match load_baseline_hash(&vector.vector_id) {
                Some(hash) => {
                    if hash != result.response_sha256 {
                        let reason = format!(
                            "baseline hash mismatch: expected {}, got {}",
                            hash, result.response_sha256
                        );
                        failures.push(reason.clone());
                        let expected = load_baseline_bytes(&vector.vector_id);
                        if let Err(e) = write_drift_artifacts(
                            &root,
                            &run_id,
                            &os_id,
                            &vector.vector_id,
                            expected.as_deref(),
                            &result.response_bytes,
                            &reason,
                        ) {
                            all_failures.push(format!(
                                "{}: failed to write drift artifacts: {}",
                                vector.vector_id, e
                            ));
                        }
                    }
                }
                None => failures.push("baseline missing".to_string()),
            }
        }

        let report = vector_report(&vector, &result, failures.clone());
        if let Err(e) = write_report(&root, &run_id, &os_id, &report) {
            all_failures.push(format!("{}: report write failed: {}", vector.vector_id, e));
        }

        if mode != "sync" && !failures.is_empty() {
            let expected = load_baseline_bytes(&vector.vector_id);
            let reason = failures.join(" | ");
            if let Err(e) = write_drift_artifacts(
                &root,
                &run_id,
                &os_id,
                &vector.vector_id,
                expected.as_deref(),
                &result.response_bytes,
                &reason,
            ) {
                all_failures.push(format!(
                    "{}: failed to write drift artifacts: {}",
                    vector.vector_id, e
                ));
            }
        }

        all_failures.extend(
            failures
                .into_iter()
                .map(|f| format!("{}: {}", vector.vector_id, f)),
        );
    }

    if !all_failures.is_empty() {
        eprintln!("conformance failures:");
        for line in all_failures {
            eprintln!("- {line}");
        }
        return ExitCode::from(1);
    }
    println!("conformance runner completed successfully");
    ExitCode::SUCCESS
}
