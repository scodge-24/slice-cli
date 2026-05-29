use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("rust/slice-rs has a repo root grandparent")
        .to_path_buf()
}

fn run_python(args: &[&str]) -> (i32, Value) {
    let (status, stdout, stderr) = run_python_raw(args);
    let value = serde_json::from_slice(&stdout).unwrap_or_else(|err| {
        panic!(
            "python output was not json: {err}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
    });
    (status, value)
}

fn run_rust(args: &[&str]) -> (i32, Value) {
    let (status, stdout, stderr) = run_rust_raw(args);
    let value = serde_json::from_slice(&stdout).unwrap_or_else(|err| {
        panic!(
            "rust output was not json: {err}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
    });
    (status, value)
}

fn run_python_raw(args: &[&str]) -> (i32, Vec<u8>, Vec<u8>) {
    let root = repo_root();
    let output = Command::new("python3")
        .args(["-m", "slice_cli", "--repo", "examples/mock-repo"])
        .args(args)
        .current_dir(&root)
        .output()
        .expect("python slice command runs");
    let status = output.status.code().unwrap_or(1);
    (status, output.stdout, output.stderr)
}

fn run_rust_raw(args: &[&str]) -> (i32, Vec<u8>, Vec<u8>) {
    let root = repo_root();
    let output = Command::new(env!("CARGO_BIN_EXE_slice-rs"))
        .args(["--repo", "examples/mock-repo"])
        .args(args)
        .current_dir(&root)
        .output()
        .expect("rust slice command runs");
    let status = output.status.code().unwrap_or(1);
    (status, output.stdout, output.stderr)
}

fn run_python_for_repo(repo: &Path, args: &[&str]) -> (i32, Value) {
    let root = repo_root();
    let output = Command::new("python3")
        .args(["-m", "slice_cli", "--repo"])
        .arg(repo)
        .args(args)
        .env("PYTHONPATH", root)
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

fn run_rust_for_repo(repo: &Path, args: &[&str]) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_slice-rs"))
        .arg("--repo")
        .arg(repo)
        .args(args)
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

fn run_git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git command runs");
    assert!(
        output.status.success(),
        "git {args:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn fixture_repo() -> TempDir {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();
    std::fs::create_dir_all(repo.join("src/auth")).unwrap();
    std::fs::create_dir_all(repo.join("slices")).unwrap();
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    std::fs::write(
        repo.join("src/auth/middleware.py"),
        "def verify_token():\n    return 1\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("src/auth/sessions.py"),
        "def get_session():\n    return {}\n",
    )
    .unwrap();
    std::fs::write(repo.join("docs/auth-guide.md"), "# Auth Guide\n").unwrap();
    std::fs::write(
        repo.join("slices/auth-service.md"),
        r"---
slice_id: auth-service
description: Authentication
files:
  - src/auth/middleware.py
  - src/auth/sessions.py
abstractions: []
dependencies: []
---

## System Behavior

Auth behavior.
",
    )
    .unwrap();
    std::fs::write(
        repo.join("slices/DOCS.yaml"),
        r#"vault_root: ../docs
docs:
  auth-guide:
    path: auth-guide.md
    slices:
      - auth-service
    verified_at: ""
    tags:
      - auth
"#,
    )
    .unwrap();
    run_git(repo, &["init"]);
    run_git(repo, &["config", "user.email", "slice-rs@example.test"]);
    run_git(repo, &["config", "user.name", "slice-rs"]);
    run_git(repo, &["add", "-A"]);
    run_git(repo, &["commit", "-m", "initial"]);
    temp
}

#[test]
fn read_only_json_matches_python() {
    for args in [
        &["list", "--json"][..],
        &["show", "auth-service", "--json"],
        &["show", "auth-service", "--body", "--json"],
        &["show", "auth-service", "--system", "--json"],
        &["show", "auth-service", "--call-stacks", "--json"],
        &["show", "auth-service", "--verification", "--json"],
        &["files", "auth-service", "--json"],
        &["deps", "api-handlers", "--json"],
        &["for", "src/auth/middleware.py", "--json"],
        &["context", "src/auth/middleware.py", "--json"],
        &["context", "src/auth/middleware.py", "--strict", "--json"],
        &[
            "context",
            "src/auth/middleware.py",
            "--best-effort",
            "--json",
        ],
        &["find", "auth", "--json"],
        &["docs", "auth-service", "--json"],
        &["stale-docs", "--json"],
        &["check", "--json"],
    ] {
        assert_eq!(run_rust(args), run_python(args), "args: {args:?}");
    }
}

#[test]
fn affected_docs_matches_python_for_legacy_manifest_shape() {
    let args = ["affected-docs", "src/auth/middleware.py", "--json"];
    assert_eq!(run_rust(&args), run_python(&args));
}

#[test]
fn delegated_and_subprocess_commands_match_python() {
    for args in [
        &["sync-index", "--stdout"][..],
        &["grep", "auth-service", "verify_token"],
        &["init", "--dry-run"],
    ] {
        assert_eq!(run_rust_raw(args), run_python_raw(args), "args: {args:?}");
    }
}

#[test]
fn fingerprint_staleness_narrows_changed_files_like_python() {
    let temp = fixture_repo();
    let repo = temp.path();

    let stamp = Command::new(env!("CARGO_BIN_EXE_slice-rs"))
        .arg("--repo")
        .arg(repo)
        .args(["stamp", "auth-guide"])
        .output()
        .expect("stamp runs");
    assert!(
        stamp.status.success(),
        "stamp failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&stamp.stdout),
        String::from_utf8_lossy(&stamp.stderr)
    );

    std::fs::write(
        repo.join("src/auth/middleware.py"),
        "def verify_token():\n    return 2\n",
    )
    .unwrap();
    run_git(repo, &["add", "-A"]);
    run_git(repo, &["commit", "-m", "edit middleware"]);

    let args = ["stale-docs", "--json"];
    let rust = run_rust_for_repo(repo, &args);
    let python = run_python_for_repo(repo, &args);
    assert_eq!(rust, python);
    let changed = rust.1[0]["changed_files"].as_array().unwrap();
    assert!(changed.iter().any(|file| file == "src/auth/middleware.py"));
    assert!(!changed.iter().any(|file| file == "src/auth/sessions.py"));
}
