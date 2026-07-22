//! Test-only compilation, linkage, and execution of the installed C ABI header.

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn target_debug() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("target"))
        .join("debug")
}

fn compile_and_run(source: &str, cpp: bool) {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = target_debug();
    let scratch = target.join("header-abi-smoke");
    std::fs::create_dir_all(&scratch).unwrap();
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
    let status = Command::new(env!("CARGO"))
        .current_dir(workspace_root())
        .args(["build", "-p", "pg-ffi", "--lib"])
        .status()
        .expect("build pg-ffi cdylib");
    assert!(status.success());
    compile_and_run("header_smoke.c", false);
    compile_and_run("header_smoke.cpp", true);
}
