//! Test-only compilation, linkage, and execution of the installed C ABI header.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static SCRATCH_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn target_profile() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("target"))
        .join(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        })
}

struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new(target: &Path) -> Self {
        let sequence = SCRATCH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = target.join(format!(
            "header-abi-smoke-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create unique header smoke scratch directory");
        Self(path)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove header smoke scratch directory");
    }
}

fn compile_and_run(source: &str, cpp: bool, scratch: &Path) {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = target_profile();
    let stem = if cpp {
        "pangloss_header_cpp"
    } else {
        "pangloss_header_c"
    };
    let exe = scratch.join(format!("{stem}{}", std::env::consts::EXE_SUFFIX));
    let rustc = Command::new("rustc").arg("-vV").output().unwrap();
    let version = String::from_utf8(rustc.stdout).unwrap();
    let host = version
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .unwrap();
    let compiler = cc::Build::new()
        .cpp(cpp)
        .host(host)
        .target(host)
        .opt_level(0)
        .debug(false)
        .cargo_metadata(false)
        .get_compiler();
    let mut command = compiler.to_command();
    if compiler.is_like_msvc() {
        command
            .arg("/nologo")
            .arg("/WX")
            .arg("/W4")
            .arg(format!("/I{}", manifest.join("include").display()))
            .arg(manifest.join("tests").join(source))
            .arg(target.join("pangloss_ffi.dll.lib"))
            .arg(format!(
                "/Fo:{}",
                scratch.join(format!("{stem}.obj")).display()
            ))
            .arg(format!("/Fe:{}", exe.display()));
    } else {
        command
            .arg("-Werror")
            .arg("-Wall")
            .arg("-Wextra")
            .arg("-I")
            .arg(manifest.join("include"))
            .arg(manifest.join("tests").join(source))
            .arg("-L")
            .arg(&target)
            .arg("-lpangloss_ffi")
            .arg("-o")
            .arg(&exe);
    }
    let status = command.status().expect("run native compiler");
    assert!(
        status.success(),
        "header smoke compiler failed: {command:?}"
    );

    #[cfg(target_os = "windows")]
    std::fs::copy(
        target.join("pangloss_ffi.dll"),
        scratch.join("pangloss_ffi.dll"),
    )
    .unwrap();
    let status = Command::new(&exe)
        .status()
        .expect("run linked header smoke");
    assert!(status.success(), "linked header smoke failed: {exe:?}");
}

#[test]
fn installed_header_compiles_links_and_runs_as_c_and_cpp() {
    let mut build = Command::new(env!("CARGO"));
    build
        .current_dir(workspace_root())
        .args(["build", "-p", "pg-ffi", "--lib"]);
    if !cfg!(debug_assertions) {
        build.arg("--release");
    }
    let status = build.status().expect("build pg-ffi cdylib");
    assert!(status.success());
    let scratch = ScratchDir::new(&target_profile());
    compile_and_run("header_smoke.c", false, &scratch.0);
    compile_and_run("header_smoke.cpp", true, &scratch.0);
}

#[test]
fn scratch_directories_are_unique_and_removed_on_drop() {
    let target = target_profile();
    let first = ScratchDir::new(&target);
    let first_path = first.0.clone();
    let second = ScratchDir::new(&target);
    let second_path = second.0.clone();
    assert_ne!(first_path, second_path);
    assert!(first_path.is_dir());
    assert!(second_path.is_dir());
    drop(first);
    drop(second);
    assert!(!first_path.exists());
    assert!(!second_path.exists());
}
