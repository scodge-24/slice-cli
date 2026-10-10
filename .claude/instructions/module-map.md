## Module map

| I want to change... | Look in (`rust/slice-rs/src/`) |
|---------------------|--------------------------------|
| repo / git / path discovery | `context.rs`, `git_backend.rs` |
| path normalization + globs | `paths.rs` |
| slice parsing + fingerprinting | `slices.rs`, `index.rs` |
| doc manifest load/save | `manifest.rs` |
| command handlers (list/show/stale/affected/stamp/…) | `commands.rs` |
| symbol spans (outline/symbols; heuristic + AST backends) | `symbols.rs`, `symbols_ast.rs` |
| semantic index + `locate` / `find --semantic` (feature `semantic`) | `semantic.rs` |
| validation (`slice check`) + verification links | `check.rs` |
| config (context ambiguity) | `config.rs` |
| CLI wiring / arg parsing | `cli.rs` |
| data types | `models.rs` |
| errors / exit codes | `error.rs`, `main.rs` |
