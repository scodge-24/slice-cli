# Python baseline benchmarks

Ad-hoc latency measurements of the Python `slice` CLI, recorded 2026-05-29 to
inform the "is Python fast enough, or port to Rust?" decision
(see `design/platform-evaluation.md`). These are real-repo numbers to complement
the synthetic scale harness in `bench/run.py`.

**Headline:** every invocation is ~250ms, of which ~70–85% is Python interpreter
startup + imports, not actual work. Slice count and repo size barely move it.

## Environment

- Python 3.12.3, git 2.43.0
- Linux 6.6.87 (WSL2), 16 cores
- CLI under test: the single-file `slices_cli.py` (pre-refactor `main` branch at
  `/home/scodge/dev/plugins/slice-cli`). The package version in this worktree was
  not separately benchmarked — import cost is expected to be similar or slightly
  higher (more module files to import).
- Repos:
  - `examples/mock-repo` — 4 slices (toy)
  - `/home/scodge/dev/meals` — 22 slices, 281 tracked files, 468M (the real
    dogfooding repo; no `DOCS.yaml`, so staleness uses the git SHA-diff fallback)

## How these were recorded

Three techniques, all wall-clock:

1. **Single-command wall clock** — `/usr/bin/time -f "%e s" <cmd>` (or
   `-f "%e" -o /tmp/_t` then `cat` to dodge stderr-redirection clobbering the
   timing line). Reported as 3 consecutive runs, warm filesystem cache.
2. **Per-command average** — `date +%s.%N` around a `for` loop of 30 runs,
   divided by 30.
3. **Session aggregate** — `date +%s.%N` around a shell function that issues a
   realistic sequence of `slice` calls (see "Agent session model" below).

`import`-cost attribution used `python3 -X importtime -c 'import slices_cli'`
(cumulative microseconds per module).

Caveats: warm-cache, single machine, not isolated from background load. Numbers
are ±10–20ms run to run; treat them as orders of magnitude, not exact constants.
Timings exclude the Bash-tool / harness round-trip an agent pays per call (that
overhead is language-independent and sits on top of these numbers).

## 1. Startup + import breakdown (the fixed tax)

Measured on `slices_cli.py` (single file):

| What | Time |
|------|------|
| bare `python3 -c 'pass'` | ~30–40ms |
| `python3 -S -c 'pass'` (skip site init) | ~10–20ms |
| `python3 -c 'import yaml'` | ~70ms |
| `python3 -c 'import slices_cli'` (full module) | ~170–190ms |

`-X importtime` cumulative cost of the heaviest imports:

| Module | Cumulative |
|--------|-----------|
| `slices_cli` (total) | ~114ms |
| `yaml` | 27ms |
| `dataclasses` (+`inspect`) | 17ms |
| `site` (skippable with `-S`) | 16ms |
| `argparse` | 12ms |
| `subprocess` | 10ms |
| `pathlib` | 9ms |
| `typing`, `re` | ~7ms each |

Takeaway: most of the import cost is unavoidable stdlib + pyyaml that nearly
every command needs. `-S` + lazy-importing `subprocess`/`shutil` on read-only
commands could shave ~30–50ms; nothing in Python gets this near zero.

## 2. Per-command latency

Mock repo (4 slices), 3 runs each:

| Command | Times |
|---------|-------|
| `list --json` | 0.21 / 0.24 / 0.25 s |
| `check` | 0.26 / 0.26 / 0.27 s |
| `context <file>` | 0.26 / 0.26 / 0.26 s |

Meals repo (22 slices, 281 files, 468M), 3 runs each:

| Command | Times |
|---------|-------|
| `list --json` | 0.24 / 0.25 / 0.25 s |
| `context <file>` | 0.29 / 0.29 / 0.24 s |
| `affected-docs` (1 file) | 0.27 / 0.24 / 0.24 s |
| `affected-docs` (10 files, batched) | 0.25 / 0.29 / 0.32 s |
| `check` (validate 22 slices + git) | 0.28 / 0.31 / 0.33 s |
| `stale-docs` | 0.26 / 0.25 / 0.29 s |

**Scaling is a non-issue:** 4 slices → 22 slices added only ~10–40ms per command.
The 468M repo size barely registers (git ops are targeted, not full-tree).
**Batching is nearly free:** `affected-docs` over 10 files costs ~the same as over
1 — the per-file work is negligible; it's all startup.

## 3. Agent session aggregates

"Agent session model" mirrors the per-edit loop in `_AGENT_INSTRUCTIONS`
(`slice context <file>` before editing → `slice affected-docs <files>` after →
`slice stale-docs` → `slice check`):

- Moderate = 5×`context` + 5×`affected-docs` + 1×batch `affected-docs` +
  `stale-docs` + `check` = **13 calls**
- Heavy = moderate ×3 = **39 calls**
- Batched = 5×`context` + 1×batch `affected-docs` + `stale-docs` + `check` =
  **8 calls**

| Session | Calls | Mock repo | Meals repo |
|---------|-------|-----------|------------|
| Moderate | 13 | 2.32s | 3.38s |
| Heavy | 39 | 7.45s | 10.55s |
| Moderate (batched) | 8 | — | 1.99s |

Batching the 5 separate `affected-docs` calls into 1 cut the moderate meals
session 3.38s → 1.99s (−41%) — a prompt-string change, no code.

## 4. git subprocess cost (floor for staleness commands)

Each `git` invocation on the meals repo (avg of 20 runs). This cost is OS-level
process spawn — identical whether the caller is Python or Rust, so it bounds any
command that shells out to git unless a native git library (e.g. gitoxide) is
used instead.

| git call | Cost |
|----------|------|
| `rev-parse HEAD` | 4.02 ms |
| `diff --name-only HEAD` | 5.78 ms |
| `status --porcelain` | 6.82 ms |
| `ls-files` | 4.43 ms |

## 5. Rust spike comparison (context for the port decision)

A dependency-free std-Rust reimplementation of `list --json` (faithful: identical
output for all 22 meals slices — same ids, loc, descriptions), `rustc -O`,
avg of 30 runs:

| `list --json` (meals) | avg |
|-----------------------|-----|
| Rust (std only) | **3.33 ms** |
| Python | **271.86 ms** |

~82x. Confirms <5ms is achievable for the read/parse hot path. git-touching
commands would floor at ~8–16ms with subprocess git (still ~20–35x), or <5ms only
by replacing git invocation with gitoxide. Spike source kept at
`/tmp/slice-rs-spike/list.rs` (throwaway).
