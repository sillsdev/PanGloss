//! Test-support-only executable for `tests/worker_execution_limits_contract.rs`: a minimal process wrapper around `run_worker_child` so tests can exercise supervisor timeout, crash, and malformed selected-payload behavior with a real child process. Never part of any shipped product surface. The env vars below are read only by this binary's `main` (never by production code).

// Cargo auto-discovers this bin target for every check target including wasm32, so give wasm32 a trivial no-op main rather than pulling the non-wasm-only pg_foma::worker dependency into that build.
#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if std::env::var("PANGLOSS_WORKER_TEST_CRASH").as_deref() == Ok("1") {
        std::process::exit(97);
    }
    if let Ok(ms) = std::env::var("PANGLOSS_WORKER_TEST_SLEEP_MS") {
        if let Ok(ms) = ms.parse::<u64>() {
            std::thread::sleep(std::time::Duration::from_millis(ms));
        }
    }

    if let Ok(mode) = std::env::var("PANGLOSS_WORKER_TEST_OUTPUT_MODE") {
        if let Err(e) = write_synthetic_selected_output(&mode) {
            eprintln!("worker_test_child: synthetic output error: {e}");
            std::process::exit(2);
        }
        return;
    }

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    if let Err(e) = pg_foma::worker::run_worker_child(stdin.lock(), stdout.lock()) {
        eprintln!("worker_test_child: I/O error: {e}");
        std::process::exit(2);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn write_synthetic_selected_output(mode: &str) -> std::io::Result<()> {
    use pg_foma::completed_build::{CompletedBackendBuildWire, CompletionProofWire};
    use pg_foma::worker::{CompileWorkerOutcome, CompileWorkerResult, WORKER_PROTOCOL_VERSION};
    use sha2::{Digest, Sha256};
    use std::io::Write;

    let payload = b"fst!";
    let mut digest = Sha256::new();
    digest.update(payload);
    let payload_sha256 = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let build = CompletedBackendBuildWire {
        requested_strategy: "templated-underlying-tokens".to_string(),
        realized_strategy: "templated-underlying-tokens".to_string(),
        grammar_identity: "grammar".to_string(),
        attempt_id: "attempt-test".to_string(),
        completion_proof: CompletionProofWire::TemplatedFullEmission {
            uncovered_count: 0,
            skipped_count: 0,
        },
        state_count: 1,
        arc_count: 1,
        model_fingerprint: "model".to_string(),
        payload_fingerprint: payload_sha256.clone(),
    };
    let result = CompileWorkerResult {
        protocol_version: WORKER_PROTOCOL_VERSION,
        outcome: CompileWorkerOutcome::SelectedSuccess {
            build,
            payload_byte_len: payload.len() as u64,
            payload_sha256,
        },
    };
    let result_json = serde_json::to_vec(&result).expect("synthetic result serializes");
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    write_frame(&mut stdout, &result_json)?;

    match mode {
        "selected-valid" => {
            write_frame(&mut stdout, payload)?;
        }
        "selected-missing" => {}
        "selected-truncated" => {
            stdout.write_all(&(payload.len() as u64).to_le_bytes())?;
            stdout.write_all(&payload[..payload.len() - 1])?;
            stdout.flush()?;
        }
        "selected-trailing" => {
            write_frame(&mut stdout, payload)?;
            stdout.write_all(b"trailing")?;
            stdout.flush()?;
        }
        "selected-stall" => {
            stdout.flush()?;
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
        other => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("unknown synthetic output mode {other:?}"),
            ));
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn write_frame<W: std::io::Write>(writer: &mut W, bytes: &[u8]) -> std::io::Result<()> {
    writer.write_all(&(bytes.len() as u64).to_le_bytes())?;
    writer.write_all(bytes)?;
    writer.flush()
}
