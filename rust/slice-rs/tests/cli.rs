use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use tempfile::TempDir;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("rust/slice-rs has a repo root grandparent")
        .to_path_buf()
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

fn run_rust_raw(args: &[&str]) -> (i32, Vec<u8>, Vec<u8>) {
    let root = repo_root();
    let output = Command::new(env!("CARGO_BIN_EXE_slice"))
        .args(["--repo", "examples/mock-repo"])
        .args(args)
        .current_dir(&root)
        .output()
        .expect("rust slice command runs");
    let status = output.status.code().unwrap_or(1);
    (status, output.stdout, output.stderr)
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

fn run_rust_raw_for_repo(repo: &Path, args: &[&str]) -> (i32, Vec<u8>, Vec<u8>) {
    let output = Command::new(env!("CARGO_BIN_EXE_slice"))
        .arg("--repo")
        .arg(repo)
        .args(args)
        .output()
        .expect("rust slice command runs");
    let status = output.status.code().unwrap_or(1);
    (status, output.stdout, output.stderr)
}

fn run_rust_raw_without_repo(args: &[&str], cwd: &Path) -> (i32, Vec<u8>, Vec<u8>) {
    let output = Command::new(env!("CARGO_BIN_EXE_slice"))
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("rust slice command runs");
    let status = output.status.code().unwrap_or(1);
    (status, output.stdout, output.stderr)
}

fn run_rust_raw_with_path(
    repo: &Path,
    args: &[&str],
    path: Option<&str>,
) -> (i32, Vec<u8>, Vec<u8>) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_slice"));
    command.arg("--repo").arg(repo).args(args);
    if let Some(path) = path {
        command.env("PATH", path);
    }
    let output = command.output().expect("rust slice command runs");
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

fn commit_all(repo: &Path, message: &str) {
    run_git(repo, &["add", "-A"]);
    run_git(repo, &["commit", "-m", message]);
}

fn stdout_text(result: &(i32, Vec<u8>, Vec<u8>)) -> String {
    String::from_utf8_lossy(&result.1).into_owned()
}

fn stderr_text(result: &(i32, Vec<u8>, Vec<u8>)) -> String {
    String::from_utf8_lossy(&result.2).into_owned()
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
    std::fs::write(
        repo.join("docs/auth-guide.md"),
        "---\ndoc_id: auth-guide\n---\n# Auth Guide\n",
    )
    .unwrap();
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
fn read_only_json_outputs_are_native_snapshots() {
    assert_eq!(
        run_rust(&["list", "--json"]),
        (
            0,
            json!([
                {"description":"API endpoint handlers and routing","doc_count":1,"loc":30,"slice_id":"api-handlers"},
                {"description":"Authentication and session management","doc_count":1,"loc":45,"slice_id":"auth-service"},
                {"description":"Core data model definitions","doc_count":1,"loc":12,"slice_id":"data-model"}
            ])
        )
    );
    assert_eq!(
        run_rust(&["show", "auth-service", "--json"]).1["docs"][0],
        json!({"doc_id":"auth-guide","path":"auth-guide.md","tags":["auth","middleware","security"],"verified_at":"57e4d1a4caf7"})
    );
    assert_eq!(
        run_rust(&["show", "auth-service", "--body", "--json"]).1["slice_id"],
        "auth-service"
    );
    assert_eq!(
        run_rust(&["show", "auth-service", "--system", "--json"]).1["sections"]["Runtime Flows"],
        "request -> require_auth -> verify_token -> get_session -> handler"
    );
    assert_eq!(
        run_rust(&["show", "auth-service", "--call-stacks", "--json"]),
        (
            0,
            json!({"sections":{"Runtime Flows":"request -> require_auth -> verify_token -> get_session -> handler"},"slice_id":"auth-service"})
        )
    );
    assert_eq!(
        run_rust(&["show", "auth-service", "--verification", "--json"]).1["sections"]["Verification"],
        "- `verify_token` <- tests/test_auth.py::test_verify_valid_token, tests/test_auth.py::test_verify_empty_token_rejected\n- `require_auth` <- tests/test_auth.py::test_require_auth_blocks_unauthenticated\n- `create_session` <- tests/test_sessions.py::test_create_and_get_session\n- `get_session` <- tests/test_sessions.py::test_create_and_get_session\n- `destroy_session` <- tests/test_sessions.py::test_destroy_session_removes_it\n- upstream: docs/auth-guide.md"
    );
    assert_eq!(
        run_rust(&["files", "auth-service", "--json"]),
        (0, json!(["src/auth/middleware.py", "src/auth/sessions.py"]))
    );
    assert_eq!(
        run_rust(&["deps", "api-handlers", "--json"]),
        (
            0,
            json!({"dependencies":["auth-service","data-model"],"mode":"direct","slice_id":"api-handlers"})
        )
    );
    assert_eq!(
        run_rust(&["deps", "api-handlers", "--transitive", "--json"]).1["mode"],
        "transitive"
    );
    assert_eq!(
        run_rust(&["for", "src/auth/middleware.py", "--json"]),
        (
            0,
            json!([{"description":"Authentication and session management","slice_id":"auth-service"}])
        )
    );
    for args in [
        &["context", "src/auth/middleware.py", "--json"][..],
        &["context", "src/auth/middleware.py", "--strict", "--json"],
        &[
            "context",
            "src/auth/middleware.py",
            "--best-effort",
            "--json",
        ],
    ] {
        let output = run_rust(args);
        assert_eq!(output.0, 0, "args: {args:?}");
        assert_eq!(output.1["slices"][0]["slice_id"], "auth-service");
        assert_eq!(output.1["slices"][0]["docs"][0]["stale"], true);
    }
    assert_eq!(
        run_rust(&["find", "auth", "--json"]).1[1],
        json!({"description":"Authentication and session management","matches":["slice_id","description","files","abstractions","doc_tags","body"],"slice_id":"auth-service"})
    );
    assert_eq!(
        run_rust(&["docs", "auth-service", "--json"]).1[0]["stale"],
        true
    );
    assert_eq!(
        run_rust(&["stale-docs", "--json"])
            .1
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let check = run_rust(&["check", "--json"]);
    assert_eq!(check.0, 0);
    assert_eq!(check.1["ok"], true);
    assert_eq!(check.1["warnings"].as_array().unwrap().len(), 4);
}

#[test]
fn affected_docs_for_legacy_manifest_shape() {
    let args = ["affected-docs", "src/auth/middleware.py", "--json"];
    assert_eq!(
        run_rust(&args),
        (
            1,
            json!([{"changed_files":["examples/mock-repo/src/auth/middleware.py","examples/mock-repo/src/auth/sessions.py"],"doc_id":"auth-guide","matching_slices":["auth-service"],"path":"auth-guide.md","status":"stale"}])
        )
    );
}

#[test]
fn read_only_human_outputs_are_native_snapshots() {
    let list = run_rust_raw(&["list"]);
    assert_eq!(list.0, 0);
    assert!(stdout_text(&list).contains("auth-service  Authentication and session management"));

    let show = run_rust_raw(&["show", "auth-service"]);
    assert_eq!(show.0, 0);
    assert!(stdout_text(&show).contains("slice_id: auth-service"));
    assert!(stdout_text(&show).contains("docs:"));

    let body = run_rust_raw(&["show", "auth-service", "--body"]);
    assert_eq!(body.0, 0);
    assert!(stdout_text(&body).contains("## System Behavior"));

    let system = run_rust_raw(&["show", "auth-service", "--system"]);
    assert_eq!(system.0, 0);
    assert!(stdout_text(&system).contains("System Behavior:"));
    assert!(stdout_text(&system).contains("Update Triggers:"));

    let call_stacks = run_rust_raw(&["show", "auth-service", "--call-stacks"]);
    assert_eq!(call_stacks.0, 0);
    assert!(stdout_text(&call_stacks).contains("Runtime Flows:"));

    let verification = run_rust_raw(&["show", "auth-service", "--verification"]);
    assert_eq!(verification.0, 0);
    assert!(stdout_text(&verification).contains("tests/test_auth.py::test_verify_valid_token"));

    assert_eq!(
        stdout_text(&run_rust_raw(&["files", "auth-service"])),
        "src/auth/middleware.py\nsrc/auth/sessions.py\n"
    );
    assert_eq!(
        stdout_text(&run_rust_raw(&["deps", "api-handlers"])),
        "auth-service\ndata-model\n"
    );
    assert!(
        stdout_text(&run_rust_raw(&["deps", "auth-service", "--reverse"])).contains("api-handlers")
    );
    assert!(
        stdout_text(&run_rust_raw(&["for", "src/auth/middleware.py"]))
            .contains("auth-service\tAuthentication and session management")
    );
    assert!(stdout_text(&run_rust_raw(&["find", "auth"])).contains("auth-service"));
    assert!(stdout_text(&run_rust_raw(&["docs", "auth-service"])).contains("[STALE] auth-guide"));
}

#[test]
fn subprocess_commands_and_init_dry_runs_are_native_snapshots() {
    let index = run_rust_raw(&["sync-index", "--stdout"]);
    assert_eq!(index.0, 0);
    assert!(stdout_text(&index).contains("# Slice Index"));
    assert!(
        stdout_text(&index)
            .contains("| `auth-service` | Authentication and session management | ~45 |")
    );

    let grep = run_rust_raw(&["grep", "auth-service", "verify_token"]);
    assert_eq!(grep.0, 0);
    assert!(stdout_text(&grep).contains("src/auth/middleware.py"));

    assert_eq!(
        stdout_text(&run_rust_raw(&["init", "--dry-run"])),
        "would write: CLAUDE.md\n"
    );
    assert_eq!(
        stdout_text(&run_rust_raw(&["init", "--agent", "--dry-run"])),
        "would write: CLAUDE.md\nwould write: .claude/skills/slice-codebase/SKILL.md\nwould write: .claude/agents/codebase-slicer.md\n"
    );
}

#[test]
fn native_write_commands_have_expected_observable_behavior() {
    let temp = fixture_repo();
    let repo = temp.path();

    let index = run_rust_raw_for_repo(repo, &["sync-index", "--stdout"]);
    assert_eq!(index.0, 0);
    assert!(stdout_text(&index).contains("| `auth-service` | Authentication | ~? |"));

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
fn native_docs_bootstrap_dry_run_and_write_have_expected_shape() {
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
    let dry_run = run_rust_raw_for_repo(repo, &["docs-bootstrap", &vault_arg, "--dry-run"]);
    assert_eq!(dry_run.0, 0);
    assert!(stdout_text(&dry_run).contains("docs found: 1"));
    assert!(stdout_text(&dry_run).contains("guide"));
    assert!(stdout_text(&dry_run).contains("slices: auth-service"));
    assert!(!repo.join("slices/DOCS.yaml").exists());

    let written = run_rust_raw_for_repo(repo, &["docs-bootstrap", &vault_arg]);
    assert_eq!(written.0, 0);
    let docs = run_rust_for_repo(repo, &["docs", "auth-service", "--json"]);
    assert_eq!(docs.0, 0);
    assert_eq!(docs.1[0]["doc_id"], "guide");
    assert_eq!(docs.1[0]["tags"][0], "auth");
}

#[test]
fn native_check_json_reports_manifest_errors() {
    let temp = fixture_repo();
    let repo = temp.path();
    assert_eq!(run_rust_raw_for_repo(repo, &["sync-index"]).0, 0);

    let args = ["check", "--json", "--no-staleness"];
    let clean = run_rust_for_repo(repo, &args);
    assert_eq!(clean.0, 0);
    assert_eq!(clean.1["ok"], true);
    assert_eq!(clean.1["errors"], json!([]));

    std::fs::remove_file(repo.join("docs/auth-guide.md")).unwrap();
    let missing = run_rust_for_repo(repo, &args);
    assert_eq!(missing.0, 1);
    assert!(
        missing.1["errors"][0]
            .as_str()
            .unwrap()
            .contains("DOCS.yaml: doc missing: auth-guide")
    );
}

#[test]
fn native_check_json_reports_unknown_manifest_slice() {
    let temp = fixture_repo();
    let repo = temp.path();
    assert_eq!(run_rust_raw_for_repo(repo, &["sync-index"]).0, 0);
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
    let result = run_rust_for_repo(repo, &args);
    assert_eq!(result.0, 1);
    assert!(
        result.1["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|err| err == "DOCS.yaml: auth-guide references unknown slice: missing-slice")
    );
}

#[test]
fn native_check_json_reports_verification_links() {
    let temp = fixture_repo();
    let repo = temp.path();
    assert_eq!(run_rust_raw_for_repo(repo, &["sync-index"]).0, 0);
    std::fs::create_dir(repo.join("tests")).unwrap();
    std::fs::write(repo.join("tests/test_auth.py"), "def test_valid(): pass\n").unwrap();

    write_auth_slice_with_verification(
        repo,
        "- `verify_token` <- tests/test_auth.py::test_valid\n- upstream: src/auth/middleware.py",
        &[],
    );
    let args = ["check", "--json", "--no-staleness", "--no-doc-drift"];
    let valid = run_rust_for_repo(repo, &args);
    assert_eq!(valid.0, 0);
    assert_eq!(valid.1["ok"], true);

    write_auth_slice_with_verification(
        repo,
        "- `verify_token` <- tests/test_missing.py::test_x\n- upstream: design/missing.md",
        &[],
    );
    let invalid = run_rust_for_repo(repo, &args);
    assert_eq!(invalid.0, 1);
    let errors = invalid.1["errors"].as_array().unwrap();
    assert!(errors.iter().any(|err| {
        err.as_str()
            .unwrap()
            .contains("verification ref missing: tests/test_missing.py::test_x")
    }));
    assert!(errors.iter().any(|err| {
        err.as_str()
            .unwrap()
            .contains("verification upstream missing: design/missing.md")
    }));
}

#[test]
fn native_check_json_reports_required_verification() {
    let temp = fixture_repo();
    let repo = temp.path();
    assert_eq!(run_rust_raw_for_repo(repo, &["sync-index"]).0, 0);
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
    let result = run_rust_for_repo(repo, &args);
    assert_eq!(result.0, 0);
    assert!(result.1["warnings"].as_array().unwrap().iter().any(
        |warning| warning == "slices/auth-service.md: abstraction not verified: create_session"
    ));
}

#[test]
fn native_check_json_reports_index_and_staged_coverage() {
    let temp = fixture_repo();
    let repo = temp.path();
    assert_eq!(run_rust_raw_for_repo(repo, &["sync-index"]).0, 0);

    std::fs::write(
        repo.join("src/auth/middleware.py"),
        "def verify_token(): return False\n",
    )
    .unwrap();
    let stale_args = ["check", "--json", "--no-doc-drift"];
    let stale = run_rust_for_repo(repo, &stale_args);
    assert_eq!(stale.0, 0);
    assert!(
        stale.1["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning.as_str().unwrap().starts_with("INDEX.md stale:"))
    );

    std::fs::write(repo.join("src/unowned.py"), "print('unowned')\n").unwrap();
    run_git(repo, &["add", "src/unowned.py"]);
    let staged_args = ["check", "--json", "--no-staleness", "--no-doc-drift"];
    let staged = run_rust_for_repo(repo, &staged_args);
    assert_eq!(staged.0, 0);
    assert!(
        staged.1["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning == "staged file uncovered: src/unowned.py")
    );
}

#[test]
fn native_check_json_reports_strict_index_and_no_manifest() {
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
        let result = run_rust_for_repo(repo, args);
        assert_eq!(result.0, 0, "args: {args:?}");
        assert_eq!(result.1["errors"], json!([]), "args: {args:?}");
        assert_eq!(result.1["slice_count"], 1, "args: {args:?}");
    }
}

#[test]
fn native_check_json_reports_doc_frontmatter_and_drift_toggles() {
    let temp = fixture_repo();
    let repo = temp.path();
    assert_eq!(run_rust_raw_for_repo(repo, &["sync-index"]).0, 0);
    std::fs::write(
        repo.join("docs/auth-guide.md"),
        "---\ndoc_id: wrong-id\n---\n# Auth Guide\n",
    )
    .unwrap();
    let args = ["check", "--json", "--no-staleness"];
    let wrong_id = run_rust_for_repo(repo, &args);
    assert_eq!(wrong_id.0, 1);
    assert!(wrong_id.1["errors"].as_array().unwrap().iter().any(|err| {
        err.as_str()
            .unwrap()
            .contains("manifest key 'auth-guide' != frontmatter doc_id 'wrong-id'")
    }));

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
        let result = run_rust_for_repo(repo, args);
        assert_eq!(result.0, 0, "args: {args:?}");
        assert_eq!(result.1["errors"], json!([]), "args: {args:?}");
    }
}

#[test]
fn context_config_and_ambiguity_are_native_snapshots() {
    let temp = fixture_repo();
    let repo = temp.path();
    add_second_owner(repo);

    let ambiguous = run_rust_raw_for_repo(repo, &["context", "src/auth/middleware.py"]);
    assert_eq!(ambiguous.0, 1);
    assert!(
        stderr_text(&ambiguous).contains("ambiguous: multiple slices own src/auth/middleware.py")
    );

    std::fs::write(
        repo.join("slices/config.yaml"),
        "context:\n  ambiguity: best_effort\n",
    )
    .unwrap();
    let best_effort = run_rust_for_repo(repo, &["context", "src/auth/middleware.py", "--json"]);
    assert_eq!(best_effort.0, 0);
    assert_eq!(best_effort.1["slices"].as_array().unwrap().len(), 2);
    let strict = run_rust_raw_for_repo(repo, &["context", "src/auth/middleware.py", "--strict"]);
    assert_eq!(strict.0, 1);
    assert!(stderr_text(&strict).contains("ambiguous"));

    std::fs::write(
        repo.join("slices/config.yaml"),
        "context:\n  ambiguity: loose\n",
    )
    .unwrap();
    let invalid_config = run_rust_raw_for_repo(repo, &["context", "auth-service"]);
    assert_eq!(invalid_config.0, 2);
    assert!(stderr_text(&invalid_config).contains("invalid context.ambiguity"));
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
fn fingerprint_staleness_narrows_changed_files() {
    let temp = fixture_repo();
    let repo = temp.path();

    let stamp = Command::new(env!("CARGO_BIN_EXE_slice"))
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
    assert_eq!(rust.0, 1);
    let changed = rust.1[0]["changed_files"].as_array().unwrap();
    assert!(changed.iter().any(|file| file == "src/auth/middleware.py"));
    assert!(!changed.iter().any(|file| file == "src/auth/sessions.py"));
}

#[test]
fn robustness_errors_have_exit_code_two() {
    let temp = fixture_repo();
    let repo = temp.path();

    std::fs::write(repo.join("slices/DOCS.yaml"), "docs:\n  auth-guide: [").unwrap();
    let malformed_docs = run_rust_raw_for_repo(repo, &["check"]);
    assert_eq!(malformed_docs.0, 2);
    assert!(stderr_text(&malformed_docs).contains("slices/DOCS.yaml"));

    std::fs::write(repo.join("slices/DOCS.yaml"), "docs: {}\n").unwrap();
    std::fs::write(
        repo.join("slices/auth-service.md"),
        "---\nslice_id:\n  - bad\n---\n",
    )
    .unwrap();
    let malformed_slice = run_rust_raw_for_repo(repo, &["list"]);
    assert_eq!(malformed_slice.0, 2);
    assert!(stderr_text(&malformed_slice).contains("slices/auth-service.md"));

    let not_repo = tempfile::tempdir().unwrap();
    let not_repo_result = run_rust_raw_without_repo(&["list"], not_repo.path());
    assert_eq!(not_repo_result.0, 2);
    assert!(stderr_text(&not_repo_result).contains("not inside a git repository"));
}

#[test]
fn env_var_repo_root_is_honored() {
    let temp = fixture_repo();
    let repo = temp.path();
    let output = Command::new(env!("CARGO_BIN_EXE_slice"))
        .args(["list", "--json"])
        .current_dir(tempfile::tempdir().unwrap().path())
        .env("SLICE_REPO", repo)
        .output()
        .expect("slice runs");
    assert!(output.status.success());
    let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows[0]["slice_id"], "auth-service");
}

#[test]
fn stale_docs_help_documents_exit_codes() {
    let help = run_rust_raw(&["stale-docs", "-h"]);
    assert_eq!(help.0, 0);
    assert!(stdout_text(&help).contains("exit 1"));
}

#[test]
fn stamp_selectors_and_dirty_tree_edges() {
    let temp = fixture_repo();
    let repo = temp.path();
    std::fs::write(
        repo.join("docs/extra.md"),
        "---\ndoc_id: extra\n---\n# Extra\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("slices/DOCS.yaml"),
        "vault_root: ../docs\ndocs:\n  auth-guide:\n    path: auth-guide.md\n    slices:\n    - auth-service\n    verified_at: \"\"\n  extra:\n    path: extra.md\n    slices:\n    - auth-service\n    verified_at: \"\"\n",
    )
    .unwrap();

    let by_path = run_rust_raw_for_repo(repo, &["stamp", "--doc", "auth-guide.md"]);
    assert_eq!(by_path.0, 0);
    assert!(stdout_text(&by_path).contains("stamped auth-guide ->"));

    let by_slice = run_rust_raw_for_repo(repo, &["stamp", "--slice", "auth-service"]);
    assert_eq!(by_slice.0, 0);
    assert!(stdout_text(&by_slice).contains("stamped auth-guide ->"));
    assert!(stdout_text(&by_slice).contains("stamped extra ->"));

    std::fs::write(repo.join("src/auth/middleware.py"), "dirty\n").unwrap();
    let dirty = run_rust_raw_for_repo(repo, &["stamp", "auth-guide"]);
    assert_eq!(dirty.0, 0);

    std::fs::remove_file(repo.join("slices/DOCS.yaml")).unwrap();
    let no_manifest = run_rust_raw_for_repo(repo, &["stamp", "auth-guide"]);
    assert_eq!(no_manifest.0, 2);
    assert!(stderr_text(&no_manifest).contains("no DOCS.yaml manifest found"));
}

#[test]
fn stamp_all_stale_and_rebase_after_stamp() {
    let temp = fixture_repo();
    let repo = temp.path();
    let initial = run_rust_raw_for_repo(repo, &["stamp", "--all"]);
    assert_eq!(initial.0, 0);
    assert!(stdout_text(&initial).contains("stamped auth-guide ->"));
    assert_eq!(
        run_rust_for_repo(repo, &["stale-docs", "--json"]),
        (0, json!([]))
    );

    std::fs::write(
        repo.join("src/auth/middleware.py"),
        "def verify_token():\n    return 99\n",
    )
    .unwrap();
    commit_all(repo, "edit middleware");
    let stale = run_rust_for_repo(repo, &["stale-docs", "--json"]);
    assert_eq!(stale.0, 1);

    let restamp = run_rust_raw_for_repo(repo, &["stamp"]);
    assert_eq!(restamp.0, 0);
    assert_eq!(
        run_rust_for_repo(repo, &["stale-docs", "--json"]),
        (0, json!([]))
    );
}

#[test]
fn legacy_sha_fallback_flags_drift() {
    let temp = fixture_repo();
    let repo = temp.path();
    std::fs::write(
        repo.join("slices/DOCS.yaml"),
        "vault_root: ../docs\ndocs:\n  auth-guide:\n    path: auth-guide.md\n    slices:\n    - auth-service\n    verified_at: deadbeef\n",
    )
    .unwrap();
    let stale = run_rust_for_repo(repo, &["stale-docs", "--json"]);
    assert_eq!(stale.0, 1);
    assert!(
        stale.1[0]["changed_files"][0]
            .as_str()
            .unwrap()
            .contains("git error")
    );
}

#[test]
fn affected_docs_current_stale_unknown_text_and_multiple_paths() {
    let temp = fixture_repo();
    let repo = temp.path();
    assert_eq!(run_rust_raw_for_repo(repo, &["stamp", "auth-guide"]).0, 0);

    let current = run_rust_for_repo(repo, &["affected-docs", "src/auth/middleware.py", "--json"]);
    assert_eq!(current.0, 0);
    assert_eq!(current.1[0]["status"], "current");

    std::fs::write(
        repo.join("src/auth/middleware.py"),
        "def verify_token():\n    return 3\n",
    )
    .unwrap();
    let stale = run_rust_for_repo(repo, &["affected-docs", "src/auth/middleware.py", "--json"]);
    assert_eq!(stale.0, 1);
    assert_eq!(stale.1[0]["status"], "stale");

    let unknown = run_rust_for_repo(repo, &["affected-docs", "src/unknown.py", "--json"]);
    assert_eq!(unknown, (0, json!([])));

    let human = run_rust_raw_for_repo(repo, &["affected-docs", "src/auth/middleware.py"]);
    assert_eq!(human.0, 1);
    assert!(stdout_text(&human).contains("[STALE] auth-guide"));

    std::fs::write(repo.join("src/api.py"), "def handle():\n    return 'ok'\n").unwrap();
    std::fs::write(
        repo.join("slices/api.md"),
        "---\nslice_id: api\ndescription: API\nfiles:\n  - src/api.py\ndependencies: []\n---\n",
    )
    .unwrap();
    std::fs::write(repo.join("docs/api.md"), "---\ndoc_id: api\n---\n# API\n").unwrap();
    std::fs::write(
        repo.join("slices/DOCS.yaml"),
        "vault_root: ../docs\ndocs:\n  auth-guide:\n    path: auth-guide.md\n    slices:\n    - auth-service\n    verified_at: \"\"\n  api:\n    path: api.md\n    slices:\n    - api\n    verified_at: \"\"\n",
    )
    .unwrap();
    let multiple = run_rust_for_repo(
        repo,
        &[
            "affected-docs",
            "src/auth/middleware.py",
            "src/api.py",
            "--json",
        ],
    );
    assert_eq!(multiple.0, 1);
    assert_eq!(multiple.1.as_array().unwrap().len(), 2);
}

#[test]
fn init_preserves_existing_agents_and_skips_skill_without_agent() {
    let temp = fixture_repo();
    let repo = temp.path();
    std::fs::write(repo.join("CLAUDE.md"), "# Existing\n").unwrap();
    std::fs::write(repo.join("AGENTS.md"), "# Agents\n").unwrap();

    let result = run_rust_raw_for_repo(repo, &["init"]);
    assert_eq!(result.0, 0);
    assert!(
        std::fs::read_to_string(repo.join("CLAUDE.md"))
            .unwrap()
            .starts_with("# Existing\n")
    );
    assert!(
        std::fs::read_to_string(repo.join("AGENTS.md"))
            .unwrap()
            .contains("slice context")
    );
    assert!(!repo.join(".claude/skills/slice-codebase/SKILL.md").exists());
}

#[test]
fn init_help_includes_examples() {
    let help = run_rust_raw(&["init", "-h"]);
    assert_eq!(help.0, 0);
    assert!(stdout_text(&help).contains("--agent"));
    assert!(stdout_text(&help).contains("Examples"));
}

#[test]
fn p2_command_edges_are_covered() {
    let temp = fixture_repo();
    let repo = temp.path();
    std::fs::write(
        repo.join("slices/worker.md"),
        "---\nslice_id: worker\ndescription: Worker\nfiles:\n  - src/auth/sessions.py\ndependencies:\n  - auth-service\n---\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("slices/auth-service.md"),
        "---\nslice_id: auth-service\ndescription: Authentication\nfiles:\n  - src/auth/middleware.py\n  - src/auth/sessions.py\ndependencies:\n  - worker\n---\n\n## System Behavior\n\nAuth behavior.\n",
    )
    .unwrap();

    let deps = run_rust_for_repo(repo, &["deps", "auth-service", "--transitive", "--json"]);
    assert_eq!(deps.0, 0);
    assert_eq!(deps.1["dependencies"], json!(["worker", "auth-service"]));

    let unknown = run_rust_raw_for_repo(repo, &["show", "missing"]);
    assert_eq!(unknown.0, 2);
    assert!(stderr_text(&unknown).contains("unknown slice: missing"));

    let no_owner = run_rust_raw_for_repo(repo, &["context", "src/no-owner.py"]);
    assert_eq!(no_owner.0, 1);
    assert!(stderr_text(&no_owner).contains("no owning slice"));

    let missing_sections = run_rust_for_repo(repo, &["context", "auth-service", "--json"]);
    assert_eq!(missing_sections.0, 0);
    assert_eq!(
        missing_sections.1["slices"][0]["sections"]
            .as_object()
            .unwrap()
            .len(),
        1
    );

    let show = run_rust_raw_for_repo(repo, &["show", "auth-service", "--verification"]);
    assert_eq!(show.0, 0);
    assert!(stdout_text(&show).contains("  (not present)"));
}

#[test]
fn include_exclude_filtering_limits_doc_drift_scope() {
    let temp = fixture_repo();
    let repo = temp.path();
    std::fs::write(
        repo.join("slices/DOCS.yaml"),
        "vault_root: ../docs\ndocs:\n  auth-guide:\n    path: auth-guide.md\n    slices:\n    - auth-service\n    include:\n    - src/auth/*.py\n    exclude:\n    - src/auth/sessions.py\n    verified_at: \"\"\n",
    )
    .unwrap();
    assert_eq!(run_rust_raw_for_repo(repo, &["stamp", "auth-guide"]).0, 0);
    std::fs::write(
        repo.join("src/auth/sessions.py"),
        "def get_session():\n    return {'changed': True}\n",
    )
    .unwrap();
    assert_eq!(
        run_rust_for_repo(repo, &["stale-docs", "--json"]),
        (0, json!([]))
    );
    std::fs::write(
        repo.join("src/auth/middleware.py"),
        "def verify_token():\n    return 4\n",
    )
    .unwrap();
    let stale = run_rust_for_repo(repo, &["stale-docs", "--json"]);
    assert_eq!(stale.0, 1);
    assert_eq!(
        stale.1[0]["changed_files"],
        json!(["src/auth/middleware.py"])
    );
}

#[test]
fn grep_without_rg_is_graceful() {
    let temp = fixture_repo();
    let repo = temp.path();
    let result = run_rust_raw_with_path(repo, &["grep", "auth-service", "verify_token"], Some(""));
    assert_eq!(result.0, 2);
    assert!(stderr_text(&result).contains("rg is required"));
}

#[test]
fn doc_drift_edges_cover_missing_sha_bad_sha_dirty_and_multi_slice() {
    let temp = fixture_repo();
    let repo = temp.path();
    std::fs::write(
        repo.join("slices/session-view.md"),
        "---\nslice_id: session-view\ndescription: Session view\nfiles:\n  - src/auth/sessions.py\ndependencies: []\n---\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("slices/DOCS.yaml"),
        "vault_root: ../docs\ndocs:\n  auth-guide:\n    path: auth-guide.md\n    slices:\n    - auth-service\n    - session-view\n    verified_at: \"\"\n",
    )
    .unwrap();

    let missing_verified = run_rust_for_repo(repo, &["stale-docs", "--json"]);
    assert_eq!(missing_verified.0, 1);
    assert_eq!(
        missing_verified.1[0]["affected_slices"],
        json!(["auth-service", "session-view"])
    );

    assert_eq!(run_rust_raw_for_repo(repo, &["stamp", "auth-guide"]).0, 0);
    std::fs::write(
        repo.join("src/auth/sessions.py"),
        "def get_session():\n    return {'dirty': True}\n",
    )
    .unwrap();
    let dirty = run_rust_for_repo(repo, &["stale-docs", "--json"]);
    assert_eq!(dirty.0, 1);
    assert!(
        dirty.1[0]["affected_slices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|slice| slice == "session-view")
    );

    std::fs::write(
        repo.join("slices/DOCS.yaml"),
        "vault_root: ../docs\ndocs:\n  auth-guide:\n    path: auth-guide.md\n    slices:\n    - auth-service\n    verified_at: badbadbad\n",
    )
    .unwrap();
    let bad_sha_check = run_rust_for_repo(repo, &["check", "--json", "--no-staleness"]);
    assert_eq!(bad_sha_check.0, 1);
    assert!(
        bad_sha_check.1["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| {
                warning
                    .as_str()
                    .unwrap()
                    .contains("git error: unable to resolve badbadbad")
            })
    );
}

#[test]
fn check_staleness_on_reports_dirty_source_worktree_and_doc_drift() {
    let temp = fixture_repo();
    let repo = temp.path();
    assert_eq!(run_rust_raw_for_repo(repo, &["sync-index"]).0, 0);
    assert_eq!(run_rust_raw_for_repo(repo, &["stamp", "auth-guide"]).0, 0);

    std::fs::write(
        repo.join("src/auth/middleware.py"),
        "def verify_token():\n    return 'dirty'\n",
    )
    .unwrap();
    let dirty = run_rust_for_repo(repo, &["check", "--json"]);
    assert_eq!(dirty.0, 0);
    let warnings = dirty.1["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|warning| warning.as_str().unwrap().starts_with("INDEX.md stale:"))
    );
    assert!(warnings.iter().any(|warning| {
        warning
            .as_str()
            .unwrap()
            .starts_with("doc stale: auth-guide")
    }));

    commit_all(repo, "dirty source becomes commit");
    let committed = run_rust_for_repo(repo, &["check", "--json"]);
    assert_eq!(committed.0, 0);
    assert!(
        committed.1["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| { warning.as_str().unwrap().contains("src/auth/middleware.py") })
    );
}

#[test]
fn context_and_show_help_are_covered() {
    let root_help = run_rust_raw(&["-h"]);
    assert_eq!(root_help.0, 0);
    assert!(stdout_text(&root_help).contains("context"));

    let context_help = run_rust_raw(&["context", "-h"]);
    assert_eq!(context_help.0, 0);
    assert!(stdout_text(&context_help).contains("Examples"));
    assert!(stdout_text(&context_help).contains("slice context src/auth/middleware.py"));

    let show_help = run_rust_raw(&["show", "-h"]);
    assert_eq!(show_help.0, 0);
    assert!(stdout_text(&show_help).contains("--system"));
    assert!(stdout_text(&show_help).contains("--verification"));
}
