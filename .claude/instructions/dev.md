## Dev

- Rust (toolchain pinned in `rust/slice-rs/rust-toolchain.toml`); `git` on PATH
  at runtime.
- Build/run: `cargo run --manifest-path rust/slice-rs/Cargo.toml -- <args>`.
- Install: `cargo install --path rust/slice-rs` (produces `slice`).
- Tests: `cargo test --manifest-path rust/slice-rs/Cargo.toml`.
- Lint: `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings`.
