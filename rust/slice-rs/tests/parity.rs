use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("rust/slice-rs has a repo root grandparent")
        .to_path_buf()
}

fn run_python(args: &[&str]) -> (i32, Value) {
    let root = repo_root();
    let output = Command::new("python3")
        .args(["-m", "slice_cli", "--repo", "examples/mock-repo"])
        .args(args)
        .current_dir(&root)
        .output()
        .expect("python slice command runs");
    let status = output.status.code().unwrap_or(1);
    let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "python output was not json: {err}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    });
    (status, value)
}

fn run_rust(args: &[&str]) -> (i32, Value) {
    let root = repo_root();
    let output = Command::new(env!("CARGO_BIN_EXE_slice-rs"))
        .args(["--repo", "examples/mock-repo"])
        .args(args)
        .current_dir(&root)
        .output()
        .expect("rust slice command runs");
    let status = output.status.code().unwrap_or(1);
    let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "rust output was not json: {err}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    });
    (status, value)
}

#[test]
fn read_only_json_matches_python() {
    for args in [
        &["list", "--json"][..],
        &["show", "auth-service", "--json"],
        &["files", "auth-service", "--json"],
        &["deps", "api-handlers", "--json"],
        &["for", "src/auth/middleware.py", "--json"],
        &["context", "src/auth/middleware.py", "--json"],
    ] {
        assert_eq!(run_rust(args), run_python(args), "args: {args:?}");
    }
}

#[test]
fn affected_docs_matches_python_for_legacy_manifest_shape() {
    let args = ["affected-docs", "src/auth/middleware.py", "--json"];
    assert_eq!(run_rust(&args), run_python(&args));
}
