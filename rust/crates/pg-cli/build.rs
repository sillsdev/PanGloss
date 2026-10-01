use std::path::{Path, PathBuf};
use std::process::Command;

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn emit_git_watch(root: &Path, path: &str) {
    if let Some(path) = git(root, &["rev-parse", "--git-path", path]) {
        let path = PathBuf::from(path);
        let path = if path.is_absolute() {
            path
        } else {
            root.join(path)
        };
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

fn repository_root(manifest: &Path) -> Option<PathBuf> {
    let candidate = manifest.ancestors().nth(3)?.canonicalize().ok()?;
    if !candidate.join(".git").exists() {
        println!(
            "cargo:rerun-if-changed={}",
            candidate.join(".git").display()
        );
        return None;
    }
    let root = PathBuf::from(git(&candidate, &["rev-parse", "--show-toplevel"])?)
        .canonicalize()
        .ok()?;
    (root == candidate).then_some(root)
}

fn revision(root: &Path) -> Option<String> {
    let value = git(root, &["rev-parse", "--verify", "HEAD"])?;
    let valid_hex =
        (40..=64).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_hexdigit());
    valid_hex.then_some(value)
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR"),
    );
    let root = repository_root(&manifest);
    let build_revision = match root.as_deref() {
        Some(root) => {
            emit_git_watch(root, "HEAD");
            emit_git_watch(root, "index");
            emit_git_watch(root, "packed-refs");
            if let Some(reference) = git(root, &["symbolic-ref", "-q", "HEAD"]) {
                emit_git_watch(root, &reference);
            }
            if let Some(files) = git(root, &["ls-files", "-z", "--", "rust"]) {
                for relative in files.split('\0').filter(|path| !path.is_empty()) {
                    println!("cargo:rerun-if-changed={}", root.join(relative).display());
                }
            }
            revision(root).unwrap_or_else(|| "unknown".to_string())
        }
        None => "unknown".to_string(),
    };

    let version = std::env::var("CARGO_PKG_VERSION").expect("Cargo sets CARGO_PKG_VERSION");
    let build_info = format!("pangloss/{version}+{build_revision}");
    println!("cargo:rustc-env=PANGLOSS_BUILD_REVISION={build_revision}");
    println!("cargo:rustc-env=PANGLOSS_BUILD_INFO={build_info}");
}
