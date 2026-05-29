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
    let (status, stdout, stderr) = run_python_raw_for_repo(repo, args);
    let value = serde_json::from_slice(&stdout).unwrap_or_else(|err| {
        panic!(
            "python output was not json: {err}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
    });
    (status, value)
}

fn run_rust_for_repo(repo: &Path, args: &[&str]) -> (i32, Value) {
    let (status, stdout, stderr) = run_rust_raw_for_repo(repo, args);
    let value = serde_json::from_slice(&stdout).unwrap_or_else(|err| {
        panic!(
            "rust output was not json: {err}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
    });
    (status, value)
}

fn run_python_raw_for_repo(repo: &Path, args: &[&str]) -> (i32, Vec<u8>, Vec<u8>) {
    let root = repo_root();
    let output = Command::new("python3")
        .args(["-m", "slice_cli", "--repo"])
        .arg(repo)
        .args(args)
        .env("PYTHONPATH", root)
        .output()
        .expect("python slice command runs");
    let status = output.status.code().unwrap_or(1);
    (status, output.stdout, output.stderr)
}

fn run_rust_raw_for_repo(repo: &Path, args: &[&str]) -> (i32, Vec<u8>, Vec<u8>) {
    let output = Command::new(env!("CARGO_BIN_EXE_slice-rs"))
        .arg("--repo")
        .arg(repo)
        .args(args)
        .output()
        .expect("rust slice command runs");
    let status = output.status.code().unwrap_or(1);
    (status, output.stdout, output.stderr)
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

fn write_auth_slice_with_verification(repo: &Path, verification: &str, abstractions: &[&str]) {
    let abstractions_yaml = if abstractions.is_empty() {
        "abstractions: []\n".to_owned()
    } else {
        let items = abstractions
            .iter()
            .map(|item| format!("  - \"{item}\""))
            .collect::<Vec<_>>()
            .join("\n");
        format!("abstractions:\n{items}\n")
    };
    std::fs::write(
        repo.join("slices/auth-service.md"),
        format!(
            "---\nslice_id: auth-service\ndescription: Authentication\nfiles:\n  - src/auth/middleware.py\n  - src/auth/sessions.py\n{abstractions_yaml}dependencies: []\n---\n\n## Verification\n\n{verification}\n"
        ),
    )
    .unwrap();
}

fn add_second_owner(repo: &Path) {
    std::fs::write(
        repo.join("slices/auth-extra.md"),
        "---\nslice_id: auth-extra\ndescription: Extra auth view\nloc: 5\nfiles:\n  - src/auth/middleware.py\ndependencies: []\n---\nExtra slice body.\n",
    )
    .unwrap();
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
fn read_only_human_outputs_match_python() {
    for args in [
        &["list"][..],
        &["show", "auth-service"],
        &["show", "auth-service", "--body"],
        &["show", "auth-service", "--system"],
        &["show", "auth-service", "--call-stacks"],
        &["show", "auth-service", "--verification"],
        &["files", "auth-service"],
        &["deps", "api-handlers"],
        &["deps", "auth-service", "--reverse"],
        &["for", "src/auth/middleware.py"],
        &["find", "auth"],
        &["docs", "auth-service"],
    ] {
        assert_eq!(run_rust_raw(args), run_python_raw(args), "args: {args:?}");
    }
}

#[test]
fn subprocess_commands_and_init_dry_runs_match_python() {
    for args in [
        &["sync-index", "--stdout"][..],
        &["grep", "auth-service", "verify_token"],
        &["init", "--dry-run"],
        &["init", "--agent", "--dry-run"],
    ] {
        assert_eq!(run_rust_raw(args), run_python_raw(args), "args: {args:?}");
    }
}

#[test]
fn native_write_commands_match_python_observable_behavior() {
    let temp = fixture_repo();
    let repo = temp.path();

    assert_eq!(
        run_rust_raw_for_repo(repo, &["sync-index", "--stdout"]),
        run_python_raw_for_repo(repo, &["sync-index", "--stdout"])
    );

    let rust_stamp = run_rust_raw_for_repo(repo, &["stamp", "auth-guide"]);
    assert_eq!(rust_stamp.0, 0);
    assert!(String::from_utf8_lossy(&rust_stamp.1).contains("stamped auth-guide ->"));
    assert_eq!(
        run_rust_for_repo(repo, &["stale-docs", "--json"]),
        (0, Value::Array(vec![]))
    );
    let manifest = std::fs::read_to_string(repo.join("slices/DOCS.yaml")).unwrap();
    assert!(manifest.contains("fingerprint: "));
    assert!(manifest.contains("tags:\n    - auth\n"));

    let bad_stamp = run_rust_raw_for_repo(repo, &["stamp", "missing-doc"]);
    assert_eq!(bad_stamp.0, 1);
    assert!(String::from_utf8_lossy(&bad_stamp.2).contains("no doc with id 'missing-doc'"));
}

#[test]
fn native_docs_bootstrap_dry_run_and_write_match_python_shape() {
    let temp = fixture_repo();
    let repo = temp.path();
    std::fs::remove_file(repo.join("slices/DOCS.yaml")).unwrap();
    let vault = repo.join("vault");
    std::fs::create_dir(&vault).unwrap();
    std::fs::write(
        vault.join("guide.md"),
        "---\ndoc_id: guide\ntracks:\n  - src/auth/middleware.py\ntags:\n  - auth\n---\n# Guide\n",
    )
    .unwrap();

    let vault_arg = vault.to_string_lossy();
    assert_eq!(
        run_rust_raw_for_repo(repo, &["docs-bootstrap", &vault_arg, "--dry-run"]),
        run_python_raw_for_repo(repo, &["docs-bootstrap", &vault_arg, "--dry-run"])
    );
    assert!(!repo.join("slices/DOCS.yaml").exists());

    let written = run_rust_raw_for_repo(repo, &["docs-bootstrap", &vault_arg]);
    assert_eq!(written.0, 0);
    let docs = run_rust_for_repo(repo, &["docs", "auth-service", "--json"]);
    assert_eq!(docs.0, 0);
    assert_eq!(docs.1[0]["doc_id"], "guide");
    assert_eq!(docs.1[0]["tags"][0], "auth");
}

#[test]
fn native_check_json_matches_python_for_manifest_errors() {
    let temp = fixture_repo();
    let repo = temp.path();

    let args = ["check", "--json", "--no-staleness"];
    assert_eq!(
        run_rust_for_repo(repo, &args),
        run_python_for_repo(repo, &args)
    );

    std::fs::remove_file(repo.join("docs/auth-guide.md")).unwrap();
    assert_eq!(
        run_rust_for_repo(repo, &args),
        run_python_for_repo(repo, &args)
    );
}

#[test]
fn native_check_json_matches_python_for_unknown_manifest_slice() {
    let temp = fixture_repo();
    let repo = temp.path();
    std::fs::write(
        repo.join("docs/auth-guide.md"),
        "---\ndoc_id: auth-guide\n---\n# Auth Guide\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("slices/DOCS.yaml"),
        r"vault_root: ../docs
docs:
  auth-guide:
    path: auth-guide.md
    slices:
    - missing-slice
    verified_at: abc
",
    )
    .unwrap();

    let args = ["check", "--json", "--no-staleness"];
    assert_eq!(
        run_rust_for_repo(repo, &args),
        run_python_for_repo(repo, &args)
    );
}

#[test]
fn native_check_json_matches_python_for_verification_links() {
    let temp = fixture_repo();
    let repo = temp.path();
    std::fs::create_dir(repo.join("tests")).unwrap();
    std::fs::write(repo.join("tests/test_auth.py"), "def test_valid(): pass\n").unwrap();

    write_auth_slice_with_verification(
        repo,
        "- `verify_token` <- tests/test_auth.py::test_valid\n- upstream: src/auth/middleware.py",
        &[],
    );
    let args = ["check", "--json", "--no-staleness", "--no-doc-drift"];
    assert_eq!(
        run_rust_for_repo(repo, &args),
        run_python_for_repo(repo, &args)
    );

    write_auth_slice_with_verification(
        repo,
        "- `verify_token` <- tests/test_missing.py::test_x\n- upstream: design/missing.md",
        &[],
    );
    assert_eq!(
        run_rust_for_repo(repo, &args),
        run_python_for_repo(repo, &args)
    );
}

#[test]
fn native_check_json_matches_python_for_required_verification() {
    let temp = fixture_repo();
    let repo = temp.path();
    std::fs::create_dir(repo.join("tests")).unwrap();
    std::fs::write(repo.join("tests/test_auth.py"), "def test_valid(): pass\n").unwrap();
    write_auth_slice_with_verification(
        repo,
        "- `verify_token` <- tests/test_auth.py::test_valid",
        &[
            "verify_token - checks JWT",
            "create_session - makes a session",
        ],
    );

    let args = [
        "check",
        "--json",
        "--no-staleness",
        "--no-doc-drift",
        "--require-verification",
    ];
    assert_eq!(
        run_rust_for_repo(repo, &args),
        run_python_for_repo(repo, &args)
    );
}

#[test]
fn native_check_json_matches_python_for_index_and_staged_coverage() {
    let temp = fixture_repo();
    let repo = temp.path();
    assert_eq!(run_rust_raw_for_repo(repo, &["sync-index"]).0, 0);

    std::fs::write(
        repo.join("src/auth/middleware.py"),
        "def verify_token(): return False\n",
    )
    .unwrap();
    let stale_args = ["check", "--json", "--no-doc-drift"];
    assert_eq!(
        run_rust_for_repo(repo, &stale_args),
        run_python_for_repo(repo, &stale_args)
    );

    std::fs::write(repo.join("src/unowned.py"), "print('unowned')\n").unwrap();
    run_git(repo, &["add", "src/unowned.py"]);
    let staged_args = ["check", "--json", "--no-staleness", "--no-doc-drift"];
    assert_eq!(
        run_rust_for_repo(repo, &staged_args),
        run_python_for_repo(repo, &staged_args)
    );
}

#[test]
fn native_check_json_matches_python_for_strict_index_and_no_manifest() {
    let temp = fixture_repo();
    let repo = temp.path();
    std::fs::remove_file(repo.join("slices/DOCS.yaml")).unwrap();
    assert_eq!(run_rust_raw_for_repo(repo, &["sync-index"]).0, 0);
    let slice_path = repo.join("slices/auth-service.md");
    let updated = std::fs::read_to_string(&slice_path)
        .unwrap()
        .replace("description: Authentication", "description: Auth changed");
    std::fs::write(&slice_path, updated).unwrap();

    for args in [
        &["check", "--json", "--no-staleness"][..],
        &["check", "--json", "--no-staleness", "--strict-index"],
    ] {
        assert_eq!(
            run_rust_for_repo(repo, args),
            run_python_for_repo(repo, args),
            "args: {args:?}"
        );
    }
}

#[test]
fn native_check_json_matches_python_for_doc_frontmatter_and_drift_toggles() {
    let temp = fixture_repo();
    let repo = temp.path();
    std::fs::write(
        repo.join("docs/auth-guide.md"),
        "---\ndoc_id: wrong-id\n---\n# Auth Guide\n",
    )
    .unwrap();
    let args = ["check", "--json", "--no-staleness"];
    assert_eq!(
        run_rust_for_repo(repo, &args),
        run_python_for_repo(repo, &args)
    );

    std::fs::write(
        repo.join("docs/auth-guide.md"),
        "---\ndoc_id: auth-guide\n---\n# Auth\n",
    )
    .unwrap();
    assert_eq!(run_rust_raw_for_repo(repo, &["stamp", "auth-guide"]).0, 0);
    std::fs::write(
        repo.join("src/auth/middleware.py"),
        "def verify_token():\n    return 42\n",
    )
    .unwrap();
    run_git(repo, &["add", "-A"]);
    run_git(repo, &["commit", "-m", "edit after stamp"]);

    for args in [
        &["check", "--json", "--no-staleness"][..],
        &["check", "--json", "--no-staleness", "--no-doc-drift"],
    ] {
        assert_eq!(
            run_rust_for_repo(repo, args),
            run_python_for_repo(repo, args),
            "args: {args:?}"
        );
    }
}

#[test]
fn context_config_and_ambiguity_match_python() {
    let temp = fixture_repo();
    let repo = temp.path();
    add_second_owner(repo);

    assert_eq!(
        run_rust_raw_for_repo(repo, &["context", "src/auth/middleware.py"]),
        run_python_raw_for_repo(repo, &["context", "src/auth/middleware.py"])
    );

    std::fs::write(
        repo.join("slices/config.yaml"),
        "context:\n  ambiguity: best_effort\n",
    )
    .unwrap();
    assert_eq!(
        run_rust_for_repo(repo, &["context", "src/auth/middleware.py", "--json"]),
        run_python_for_repo(repo, &["context", "src/auth/middleware.py", "--json"])
    );
    assert_eq!(
        run_rust_raw_for_repo(repo, &["context", "src/auth/middleware.py", "--strict"]),
        run_python_raw_for_repo(repo, &["context", "src/auth/middleware.py", "--strict"])
    );

    std::fs::write(
        repo.join("slices/config.yaml"),
        "context:\n  ambiguity: loose\n",
    )
    .unwrap();
    assert_eq!(
        run_rust_raw_for_repo(repo, &["context", "auth-service"]),
        run_python_raw_for_repo(repo, &["context", "auth-service"])
    );
}

#[test]
fn native_init_writes_agent_block_hook_ci_and_agent() {
    let temp = fixture_repo();
    let repo = temp.path();
    let result = run_rust_raw_for_repo(repo, &["init", "--hook", "--ci", "--agent"]);
    assert_eq!(result.0, 0);

    let claude = std::fs::read_to_string(repo.join("CLAUDE.md")).unwrap();
    assert!(claude.contains("<!-- slice-cli:start -->"));
    assert!(claude.contains("slice context"));
    let hook = repo.join(".git/hooks/pre-commit");
    assert!(hook.exists());
    assert!(
        std::fs::read_to_string(&hook)
            .unwrap()
            .contains("stale-docs")
    );
    assert!(repo.join(".github/workflows/slice-staleness.yml").exists());
    let skill =
        std::fs::read_to_string(repo.join(".claude/skills/slice-codebase/SKILL.md")).unwrap();
    assert!(skill.contains("name: slice-codebase"));
    assert!(!skill.contains("slice-cli:codebase-slicer"));
    assert!(skill.contains("subagent_type: \"codebase-slicer\""));
    assert!(repo.join(".claude/agents/codebase-slicer.md").exists());

    let second = run_rust_raw_for_repo(repo, &["init"]);
    assert_eq!(second.0, 0);
    let claude = std::fs::read_to_string(repo.join("CLAUDE.md")).unwrap();
    assert_eq!(claude.matches("<!-- slice-cli:start -->").count(), 1);
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
