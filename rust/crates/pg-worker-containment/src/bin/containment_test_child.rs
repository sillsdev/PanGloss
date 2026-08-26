#[cfg(windows)]
mod windows_child {
    use std::ffi::OsString;
    use std::fs;
    use std::io::{self, Write};
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::time::Duration;
    use windows_sys::Win32::System::JobObjects::IsProcessInJob;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    pub fn run() -> Result<(), String> {
        let mut args = std::env::args_os();
        let _executable = args.next();
        let mode = args.next().ok_or_else(|| "missing fixture mode".to_string())?;
        match mode.to_string_lossy().as_ref() {
            "argv" => emit_values(args),
            "environment" => emit_environment(args),
            "cwd" => emit_value(std::env::current_dir().map_err(io_error)?.as_os_str()),
            "job" => emit_job_membership(),
            "exit" => {
                let code = args
                    .next()
                    .ok_or_else(|| "missing exit code".to_string())?
                    .to_string_lossy()
                    .parse::<i32>()
                    .map_err(|error| error.to_string())?;
                std::process::exit(code);
            }
            "spawn-holder" => spawn_holder(args),
            "hold-pipes" => hold_pipes(args),
            "spawn-allocators" => spawn_allocators(args),
            "allocate" => allocate_forever(),
            other => Err(format!("unknown fixture mode {other}")),
        }
    }

    fn emit_values(values: impl Iterator<Item = OsString>) -> Result<(), String> {
        for value in values {
            emit_value(&value)?;
        }
        Ok(())
    }

    fn emit_environment(mut keys: impl Iterator<Item = OsString>) -> Result<(), String> {
        for key in &mut keys {
            match std::env::var_os(&key) {
                Some(value) => emit_value(&value)?,
                None => println!("ABSENT"),
            }
        }
        Ok(())
    }

    fn emit_value(value: &std::ffi::OsStr) -> Result<(), String> {
        let encoded = value
            .encode_wide()
            .map(|unit| format!("{unit:04x}"))
            .collect::<Vec<_>>()
            .join(",");
        println!("UTF16:{encoded}");
        io::stdout().flush().map_err(io_error)
    }

    fn emit_job_membership() -> Result<(), String> {
        let mut in_job = 0;
        // SAFETY: a null process handle means the current process, and the output pointer is live.
        let ok = unsafe { IsProcessInJob(GetCurrentProcess(), std::ptr::null_mut(), &mut in_job) };
        if ok == 0 {
            return Err(format!("IsProcessInJob failed: {}", io::Error::last_os_error()));
        }
        println!("IN_JOB={in_job}");
        io::stdout().flush().map_err(io_error)
    }

    fn spawn_holder(mut args: impl Iterator<Item = OsString>) -> Result<(), String> {
        let ready = args.next().ok_or_else(|| "missing ready path".to_string())?;
        let late = args.next().ok_or_else(|| "missing late path".to_string())?;
        let child = Command::new(std::env::current_exe().map_err(io_error)?)
            .arg("hold-pipes")
            .arg(ready)
            .arg(late)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(io_error)?;
        println!("DESCENDANT={}", child.id());
        io::stdout().flush().map_err(io_error)?;
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    fn hold_pipes(mut args: impl Iterator<Item = OsString>) -> Result<(), String> {
        let ready = args.next().ok_or_else(|| "missing ready path".to_string())?;
        let late = args.next().ok_or_else(|| "missing late path".to_string())?;
        fs::write(Path::new(&ready), b"ready").map_err(io_error)?;
        println!("holder stdout");
        eprintln!("holder stderr");
        io::stdout().flush().map_err(io_error)?;
        io::stderr().flush().map_err(io_error)?;
        std::thread::sleep(Duration::from_secs(10));
        fs::write(Path::new(&late), b"late").map_err(io_error)
    }

    fn spawn_allocators(mut args: impl Iterator<Item = OsString>) -> Result<(), String> {
        let ready = args.next().ok_or_else(|| "missing ready path".to_string())?;
        let executable = std::env::current_exe().map_err(io_error)?;
        for _ in 0..2 {
            Command::new(&executable)
                .arg("allocate")
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .map_err(io_error)?;
        }
        fs::write(Path::new(&ready), b"ready").map_err(io_error)?;
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    fn allocate_forever() -> Result<(), String> {
        let mut allocations = Vec::new();
        loop {
            let mut page = vec![0u8; 64 * 1024];
            for offset in (0..page.len()).step_by(4096) {
                page[offset] = 1;
            }
            allocations.push(page);
            std::thread::yield_now();
        }
    }

    fn io_error(error: io::Error) -> String {
        error.to_string()
    }
}

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_child::run() {
        eprintln!("containment_test_child: {error}");
        std::process::exit(2);
    }
}

#[cfg(not(windows))]
fn main() {}
