## Layout

- `rust/slice-rs/` — the CLI (Rust). The `slice` binary builds from `src/main.rs`;
  logic lives in `src/`, one module per concern. Tests are native Rust
  (`tests/cli.rs` + module `#[cfg(test)]` units) and build throwaway git repos in
  tmp dirs.
- `examples/mock-repo/` — a self-contained demo repo the CLI runs against (`src/` mock code, `slices/` definitions + `DOCS.yaml`, `docs/` tracked docs). Run it with `slice --repo examples/mock-repo <cmd>`. This is sample data, NOT documentation about the tool.
- `docs/` — user-facing tool documentation: architecture, manifest schema, agent
  workflows, verification links. `docs/design/` holds the committed planning/evidence
  records (feature maps, incorporation plans, review records).
- `design/` — local-only planning scratch (gitignored, not published). Finalized
  records are promoted into `docs/design/`. The empirical evidence trail lives in the
  sibling `slice-cli-benchmark` repo (`FINDINGS.md`).
