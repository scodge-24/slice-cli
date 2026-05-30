# slice-cli

**Know which design docs went stale the moment you change code.**

`slice` turns a folder of "slice" documents (`slices/*.md`) into a query surface
for humans and AI agents, and tracks whether your design docs are still accurate
relative to the code they describe. A *slice* is a named region of a codebase
(its files, dependencies, and durable system notes). `DOCS.yaml` maps docs to
slices and remembers a content fingerprint of the code each doc was last verified
against — so staleness is exact, and survives commits and rebases.

## Install

`slice` is a single self-contained binary (Rust). Two ways to get it:

**Prebuilt binary** — download the archive for your platform from the
[latest release](https://github.com/scodge-24/slice-cli/releases/latest), unpack
it, and put `slice` on your PATH.

**Build from source** (needs a Rust toolchain):

```bash
git clone https://github.com/scodge-24/slice-cli && cd slice-cli
cargo install --path rust/slice-rs        # installs the `slice` binary
```

Requires `git` on PATH at runtime.

From a checkout you can also run it without installing:

```bash
cargo run --manifest-path rust/slice-rs/Cargo.toml -- --repo examples/mock-repo list --json
```

> slice-cli began as a Python tool and was ported to Rust. The Python
> implementation is preserved at tag `python-impl-final` and branch
> `package-refactor`.

## 60-second tour

This repo ships a self-contained demo under `examples/mock-repo/`. Point `slice`
at it with `--repo`:

```bash
$ slice --repo examples/mock-repo affected-docs src/auth/middleware.py
[STALE] auth-guide  (auth-guide.md)  [auth-service]
  - examples/mock-repo/src/auth/middleware.py
  - examples/mock-repo/src/auth/sessions.py
```

That's the core loop: you changed a file, and `slice` tells you exactly which doc
to review. Orient before editing with one command:

```bash
$ slice --repo examples/mock-repo context src/auth/middleware.py
slice: auth-service
description: Authentication and session management
files: src/auth/middleware.py, src/auth/sessions.py
linked docs:
  [STALE] auth-guide  (auth-guide.md)
System Behavior:
Every protected request passes through `require_auth`...
Runtime Flows:
request -> require_auth -> verify_token -> get_session -> handler
...
```

When a doc is back in sync, mark it verified:

```bash
$ slice --repo examples/mock-repo stamp auth-guide
stamped auth-guide -> 5fb503f...
```

> `examples/` is **sample data** — a mock repo for the tool to operate on, not
> documentation about `slice` itself.

## Core commands

| Command | What it does |
|---------|--------------|
| `slice context <path>` | Owning slice + system context + stale linked docs for a file |
| `slice affected-docs <files…>` | Which docs a set of changed files may have made stale |
| `slice stale-docs` | Everything currently stale (exit 1 if any — handy as a CI gate) |
| `slice stamp <doc>` | Mark a doc verified against current code |
| `slice list` / `show` / `for` / `find` / `deps` | Navigate slices |
| `slice check` | Integrity, staleness, and verification-link checks (`--require-verification` for V-model coverage) |
| `slice init` | Wire `slice` into your repo (agent instructions, optional hook/CI) |

Run `slice <command> -h` for examples and flags.

Human output (`list`/`show`/`find`/`stale-docs`) is colored when stdout is a
terminal; control it with the global `--color=auto|always|never`. Pipes and
`--json` are never colored, and `NO_COLOR` is honored.

## Use it in your own repo

```bash
slice init            # writes agent instructions into CLAUDE.md / AGENTS.md
slice init --hook     # + a pre-commit staleness reminder
slice init --ci       # + a GitHub Actions staleness check
```

`slice init` is idempotent and re-runnable. You write slice files under `slices/`
and a `DOCS.yaml` manifest mapping docs to slices; see
[`docs/`](docs/) for architecture, the manifest schema, and agent workflows.

## Generating slices with an agent

Writing `slices/*.md` by hand is optional. slice-cli ships the agent side too —
a `slice-codebase` skill that orchestrates a `codebase-slicer` subagent to scan
the repo and write slice files for you (run with `/slice-codebase` in Claude
Code). Two ways to install it, both work across machines:

**As a Claude Code plugin (managed, namespaced, auto-updates):**

```bash
claude plugin marketplace add scodge-24/slice-cli
claude plugin install slice-cli@slice-cli
# install the `slice` binary (see Install above) so the skill finds it on PATH
```

**Or bootstrap it from the CLI (no plugin system):**

```bash
slice init --agent --global      # writes the skill + agent into ~/.claude
```

`--global` makes slicing available in every repo on the machine; drop it to
install into the current repo's `.claude/` only. Either way, then run
`/slice-codebase` in the repo you want sliced. Generation needs an LSP-capable
environment (the agent uses LSP to map call graphs). Generated slices carry
call-stack mapping (`## Runtime Flows`) and V-model verification links
(`## Verification`, validated by `slice check`) by default — see
[`docs/verification-links.md`](docs/verification-links.md).

## Docs

- [`docs/architecture.md`](docs/architecture.md) — how it works
- [`docs/manifest-schema.md`](docs/manifest-schema.md) — `DOCS.yaml` reference
- [`docs/agent-workflows.md`](docs/agent-workflows.md) — agent usage patterns
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — dev setup and conventions

## License

MIT — see [LICENSE](LICENSE).
