//! Real process-tree fixture for `worker_execution_limits_contract`; never shipped.

// Keep the wasm32 auto-discovered target from pulling in the native-only worker dependency.
#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) == Some("--fixture") {
        if let Err(error) = run_fixture(&args[1..]) {
            eprintln!("worker_test_child: fixture error: {error}");
            std::process::exit(2);
        }
        return;
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
fn run_fixture(args: &[String]) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    use std::time::Duration;

    let mode = args.first().map(String::as_str).unwrap_or("");
    match mode {
        "descendant-pipe-holder" => {
            let sentinel = required_arg(args, 1, "sentinel path")?;
            let ready = required_arg(args, 2, "ready path")?;
            let mut descendant = Command::new(std::env::current_exe()?)
                .args([
                    "--fixture",
                    "pipe-holder-then-sentinel",
                    sentinel,
                    "3000",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()?;
            std::fs::write(ready, b"ready")?;
            std::thread::sleep(Duration::from_secs(5));
            let _ = descendant.wait();
        }
        "pipe-holder-then-sentinel" => {
            let sentinel = required_arg(args, 1, "sentinel path")?;
            let delay_ms = parse_arg::<u64>(args, 2, "delay milliseconds")?;
            std::thread::sleep(Duration::from_millis(delay_ms));
            std::fs::write(sentinel, b"escaped")?;
        }
        "crash-with-descendant" => {
            let sentinel = required_arg(args, 1, "sentinel path")?;
            let ready = required_arg(args, 2, "ready path")?;
            Command::new(std::env::current_exe()?)
                .args(["--fixture", "delayed-sentinel", sentinel, "750"])
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()?;
            std::fs::write(ready, b"ready")?;
            std::process::exit(97);
        }
        "delayed-sentinel" => {
            let sentinel = required_arg(args, 1, "sentinel path")?;
            let delay_ms = parse_arg::<u64>(args, 2, "delay milliseconds")?;
            std::thread::sleep(Duration::from_millis(delay_ms));
            std::fs::write(sentinel, b"escaped")?;
        }
        "aggregate-descendant-memory" => {
            let bytes = required_arg(args, 1, "allocation bytes")?;
            let sentinel = required_arg(args, 2, "sentinel path")?;
            let mut first_allocator = Command::new(std::env::current_exe()?)
                .args(["--fixture", "allocate-and-hold", bytes, "2000"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            let mut second_allocator = Command::new(std::env::current_exe()?)
                .args(["--fixture", "allocate-and-hold", bytes, "2000"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            let mut sentinel_sibling = Command::new(std::env::current_exe()?)
                .args(["--fixture", "delayed-sentinel", sentinel, "1500"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            let _ = first_allocator.wait();
            let _ = second_allocator.wait();
            let _ = sentinel_sibling.wait();
        }
        "allocate-and-hold" => {
            let bytes = parse_arg::<usize>(args, 1, "allocation bytes")?;
            let hold_ms = parse_arg::<u64>(args, 2, "hold milliseconds")?;
            allocate_touch_and_hold(bytes, hold_ms);
        }
        other => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("unknown fixture mode {other:?}"),
            ));
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn required_arg<'a>(args: &'a [String], index: usize, label: &str) -> std::io::Result<&'a str> {
    args.get(index).map(String::as_str).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("missing fixture {label}"),
        )
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_arg<T>(args: &[String], index: usize, label: &str) -> std::io::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    required_arg(args, index, label)?.parse::<T>().map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("invalid fixture {label}: {error}"),
        )
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn allocate_touch_and_hold(bytes: usize, hold_ms: u64) {
    let mut allocation = vec![0_u8; bytes];
    for offset in (0..bytes).step_by(4096) {
        allocation[offset] = (offset / 4096) as u8;
    }
    if let Some(last) = allocation.last_mut() {
        *last = 0xa5;
    }
    std::hint::black_box(&allocation);
    std::thread::sleep(std::time::Duration::from_millis(hold_ms));
    std::hint::black_box(allocation);
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
