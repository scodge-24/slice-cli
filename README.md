# slice-cli

**Query your codebase by slice: what owns this file, what's the blast radius before
you edit, where a concept lives, what the runtime flows are — and which docs went stale.**

`slice` turns a folder of "slice" documents (`slices/*.md`) into a fast query surface
for humans and AI agents. A *slice* is a named region of a codebase — its files,
dependencies, call-stack flows, and durable system notes. Ask it what owns a file,
what transitively depends on a slice before you change it, where an abstraction lives,
and how requests flow at runtime. And because `DOCS.yaml` maps docs to slices with a
content fingerprint of the code each was last verified against, it also tells you which
design docs have gone stale — exactly, surviving commits and rebases.

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

Requires `git` on PATH at runtime. `slice browse` additionally needs optional
`fzf` (>= 0.30); every other command works without it.

From a checkout you can also run it without installing:

```bash
cargo run --manifest-path rust/slice-rs/Cargo.toml -- --repo examples/mock-repo list --json
```

> slice-cli began as a Python tool and was ported to Rust. The Python
> implementation is preserved at tag `python-impl-final` and branch
> `package-refactor`.

## 60-second tour

This repo ships a self-contained demo under `examples/mock-repo/`. Point `slice`
at it with `--repo`.

**Orient on a file before you touch it** — one command tells you the owning slice,
its dependencies, and how requests flow through it:

```bash
$ slice --repo examples/mock-repo context src/auth/middleware.py
slice: auth-service
description: Authentication and session management
doc: slices/auth-service.md
files: src/auth/middleware.py, src/auth/sessions.py
dependencies:
Invariants:
- A token is valid only if unexpired AND its session still exists.
- Sessions live in memory; a process restart drops all sessions.
Runtime Flows:
request -> require_auth -> verify_token -> get_session -> handler
...
```

**Check the blast radius before you change a slice** — every slice that
(transitively) depends on it:

```bash
$ slice --repo examples/mock-repo deps auth-service --reverse --transitive
api-handlers
```

**Then there's doc staleness.** Change a file and `slice` tells you exactly which
doc to review, and you stamp it back in sync once it's updated:

```bash
$ slice --repo examples/mock-repo affected-docs src/auth/middleware.py
[STALE] auth-guide  (auth-guide.md)  [auth-service]
  - examples/mock-repo/src/auth/middleware.py
  - examples/mock-repo/src/auth/sessions.py
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
| `slice browse` | Fuzzy-pick a slice with fzf, preview it inline (needs `fzf`) |
| `slice check` | Integrity, staleness, and verification-link checks (`--require-verification` for V-model coverage) |
| `slice init` | Wire `slice` into your repo (agent instructions, optional hook/CI) |

Run `slice <command> -h` for examples and flags.

Human output (`list`/`show`/`find`/`stale-docs`) is colored when stdout is a
terminal; control it with the global `--color=auto|always|never`. Pipes and
`--json` are never colored, and `NO_COLOR` is honored.

`slice browse` opens an `fzf` picker with a live, wrapped preview pane. `enter` shows
the selected slice; the preview switches between lenses with `ctrl-o` (overview),
`ctrl-r` (runtime call-stacks), `ctrl-d` (verification links), and `ctrl-t` (reverse
deps / blast radius). `--print` emits the chosen slice id instead, for scripting:

```bash
id=$(slice browse --print) && slice show "$id"   # && guards against cancel
```

For machine consumption use `slice list --json`, not `browse`.

## Use it in your own repo

```bash
slice init            # writes agent instructions into CLAUDE.md / AGENTS.md
slice init --hook     # + a pre-commit staleness reminder
slice init --ci       # + a GitHub Actions staleness check
```

`slice init` is idempotent and re-runnable. Generate slice files under `slices/` with
`/slice-codebase` (or write them by hand).

To also track design-doc staleness, set up `DOCS.yaml` (optional):

```bash
slice init --docs docs   # generate slices/DOCS.yaml from your docs directory
slice stamp --all        # record baseline fingerprints once mappings look right
```

`slice init --docs` bootstraps real doc→slice mappings from docs whose frontmatter
carries `tracks: [<code paths a doc describes>]`, and writes a commented stub seeded
with the docs it found otherwise (add `tracks:` and re-run). See
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

- [`docs/navigating.md`](docs/navigating.md) — navigate a codebase with `slice` (human guide)
- [`docs/architecture.md`](docs/architecture.md) — how it works
- [`docs/manifest-schema.md`](docs/manifest-schema.md) — `DOCS.yaml` reference
- [`docs/agent-workflows.md`](docs/agent-workflows.md) — agent usage patterns
- [`docs/verification-links.md`](docs/verification-links.md) — canonical card syntax contract
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — dev setup and conventions

## License

MIT — see [LICENSE](LICENSE).
