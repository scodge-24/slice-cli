# Agent-Facing Surface Consolidation Plan (v2, reviewed)

Status: draft, 2026-06-06.
Supersedes: [`agent-facing-surface-consolidation-plan.md`](../../design/agent-facing-surface-consolidation-plan.md)
(original kept as the pre-review record).

> **What changed vs the original.** This revision folds in a `/plan-eng-review` pass and an
> independent Codex challenge. Eight decisions (D1–D8) reshaped it: Phase 1 is now *output-preserving*
> consolidation only (net-new behaviour deferred); a new **Phase 0.5 precondition gate** tests the
> premise on data we already have *before* any build; Phase 3 keeps the file-precision gate but
> **pre-registers its statistics**; and Phase 1 carries an explicit test matrix incl. a critical
> regression guard. A change-log lives at the bottom, above the review report.

## Goal

Define and test a smaller agent-facing `slice` tool suite that accompanies grep instead of replacing
it. The human CLI stays broad and explicit; the agent surface exposes purpose-level tools with clear
responsibilities, so the model does not have to choose between many overlapping mechanisms.

Success means an agent can combine exact grep, slice orientation, scoped follow-up, structural ranges,
related-code expansion, and doc freshness without being steered into a generic "better search" loop —
**and** that we can show this is a real effect, not stochastic noise around a near-ceiling base.

## Starting Evidence

The current benchmark wrapper exposes the raw CLI topology almost 1:1:

- Base slice tools: `slice_list`, `slice_find`, `slice_context`, `slice_for`, `slice_show`,
  `slice_files`, `slice_deps`, `slice_grep`, `slice_docs`.
- Gated Stage 2 tools: `slice_outline`, `slice_symbols`.
- Gated semantic tool: `slice_semantic`.

That is 9–12 slice-specific choices before `read_file`, `list_dir`, `done`, and the baseline
`search_text`. The feature map flags the live problem: the **locate** cluster is crowded (`find`,
semantic find, grep, baseline search), and the **orient/relate** cluster overlaps (`context`, `for`,
`files`, `show`, `deps`).

The benchmark findings constrain the direction, and two of them are load-bearing for this plan:

- SWE-bench-style causal fix localization is usually easy for plain ripgrep — issues name the symbol,
  error string, or behaviour directly.
- Slice's validated edge is modest but real: tighter file precision and fine-grained coverage, not
  broad "find more files" superiority.
- Peer semantic search and inline-collaborator affordances **regressed** the agent loop (a louder
  retrieval mechanism, a noisier breadth list).
- Stage 2 orientation helped because it answered a *different* question ("what are the definitions and
  ranges here?").

> **⚠ Premise honesty (from the 2026-06-06 falsification of `idealised-paths.md`).** The "agent can't
> *see* collaborators" detractor was tested (`--gate-affordances` inlined them into `slice_context`)
> and **failed** (xarray 0.90 → 0.70). Base already reads the gold collaborator 4/5 seeds; base is
> **near its ceiling**; the residual gap **looks model/budget-bound**. The only empirically supported
> direction left is **consolidation** (fewer/cleaner tools). That is what this plan tests — and because
> the gap is budget-bound, the realistic upside is *reduced routing tax / noise at equal recall*, not a
> coverage jump. This plan must not over-promise a recall win.

So the next move is not a stronger universal search tool. It is a smaller companion surface around grep
and read decisions — **conditional on first showing the surface is actually a problem** (Phase 0.5).

## Non-Negotiable Design Constraints

1. **Do not replace grep.** Exact symbol / traceback / error / config key / known string → whole-repo
   grep is the primary first move.
2. **Separate purpose from mechanism.** Agent tools answer jobs ("orient this file", "show related
   files", "show definitions"), not one peer tool per implementation lens.
3. **Keep semantic internal.** Embeddings may be an internal fallback for concept location, never a
   peer agent tool unless a loop-level A/B proves it.
4. **Preserve human CLI richness.** This changes the agent facade first; it does not remove CLI commands
   from human use.
5. **Stay neutral in evals.** Tool descriptions document capability and composition, never benchmark
   scoring and never "prefer slice over your own judgement."
6. **Measure trajectory behaviour, not only offline recall.** Offline wins are insufficient if the loop
   reads worse context or stops using grep appropriately.
7. **(NEW) Earn the build.** Do not build the facade until Phase 0.5 shows v1 *losing* trajectories
   carry measurable tool-routing waste. If they do not, consolidation is solving an imagined cause.
8. **(NEW) Power the gates.** Any Phase 3 success/kill threshold that rides on a small effect must be
   pre-registered (seeds, model pin, effect size + CI, stratification) or it is not a gate.

## Proposed Agent Surface

Whole-repo grep + a smaller set of slice companion tools.

### Always-available generic tools (not slice tools; stay visible)

- `search_text(pattern, ignore_case=false)` — whole-repo ripgrep for exact terms.
- `read_file(path, start_line, end_line)` — source-grounded reading (the measured retrieval).
- `list_dir(path)` — filesystem fallback.

### Slice companion tools

The six purpose tools below are the *target* surface. **Phase 1 ships them as output-preserving
pass-throughs** (see the "Phase 1 contract" column in the mapping table); the richer behaviour in
*italics* is **deferred to Phase 1b** and gated on Phase 1 holding.

#### 1. `slice_locate(query, intent="concept")`
Map vague concepts / subsystem names / "I don't know the file" language to candidate slices. Not for
exact symbol/error lookup — use `search_text`.
- **Phase 1:** thin pass-through to `slice find <query>` (keyword over cards). Output = `find` output.
- **Inventory probe preserved (R2-#4):** hiding `slice_list` must not delete the "what areas exist?"
  capability — it is the cheapest first move on an unfamiliar repo. `slice_locate(intent="inventory")`
  (query omitted/empty) passes through to `slice list`. This keeps the surface at six tools rather than
  adding a `slice_inventory` escape hatch, and Phase 0.5 must confirm the inventory probe is actually
  used before we rely on this fold.
- *Phase 1b (deferred):* blend card-keyword + card-semantic + code-semantic + inventory ranking.*
- Hides from the agent surface: `slice_list` (folded into `intent="inventory"`), `slice_find`,
  `slice_semantic`.

#### 2. `slice_orient(target)`
Explain where a file or slice lives, after grep/locate/a path reference. The normal first slice call
after `search_text` lands on a file.
- **Phase 1:** pass-through to `slice context <target> --best-effort`. **Output byte-for-byte = today's
  `context`** (which already emits one-line `depends-on:`/`blast-radius:` summaries). **It MUST NOT
  inline a collaborator dump** — that is the falsified `--gate-affordances` behaviour (T1 guards it).
- *Phase 1b (deferred):* bundle abstractions + entry points + call-stack + docs-staleness into the
  orient payload (a behavioural change — measured separately so it cannot confound the consolidation
  A/B).*
- Hides: `slice_context`, `slice_for`, `slice_files`, the summary parts of `slice_show`, `slice_docs`
  summary mode.
- **Known tension (Codex):** `for`/`files`/`context`/`show`/`docs` are cheap probes at *different* cost
  levels; collapsing them risks replacing several cheap calls with one medium call. Phase 0.5 must check
  whether agents actually use the cheap probes as cheap probes before we hide them.

#### 3. `slice_grep(selector, pattern, ignore_case=false)`
Exact grep inside a slice-derived scope. Accompanies whole-repo grep; does not replace it.
- **Phase 1:** pass-through to `slice grep <selector> <pattern> --symbols`, owned-files only (today's
  behaviour: one slice's files, enclosing-symbol spans kept).
- **Schema honesty (R2-#5):** Phase 1 **omits the `scope` parameter from the JSON schema entirely**
  (owned is the only behaviour). Do NOT advertise `scope=forward|reverse|neighborhood` until Phase 1b
  implements them — an exposed-but-unhonored enum makes the model request scopes the adapter rejects,
  and the A/B then measures schema friction as if it were design noise.
- *Phase 1b (deferred, net-new logic):* re-introduce `scope=owned|forward|reverse|neighborhood` —
  resolve the dep closure to a file set, rg over it, **cap + label** the file set. This is NOT a
  mechanical adapter; it is new code with a real perf/output surface (see Failure Modes).*

#### 4. `slice_related(selector, direction="both", depth="direct", files=true)`
Show connected code when the task is cross-file / impact-oriented. **Never auto-inlined into orient**
(prior inline-collaborator tests added noise and displaced a working process — falsified).
- **Phase 1:** pass-through to `slice deps <selector>` with `--reverse`/`--transitive`/`--files`
  mapped from `direction`/`depth`/`files`. Output = `deps` output.
- Hides: `slice_deps`.

#### 5. `slice_structure(target)`
Turn a file or slice into readable line ranges, after a file is identified.
- **Phase 1:** target-type fork — file → `slice outline <file>`; slice → `slice symbols <slice>`.
  Output = the respective command.
- Hides: `slice_outline`, `slice_symbols`.

#### 6. `slice_freshness(target_or_changed_files)`
Doc-code freshness, impact notes, stale design context. Part of slice's moat, not a retrieval
optimization.
- **Phase 1:** input-type fork — path/slice → `slice docs <slice>`; changed-file list →
  `slice affected-docs <files…>`.
- **Scope decision (R2-#6, resolves Open Q#2):** `slice_freshness` is a **product surface only — it is
  NOT registered in any Phase 3 retrieval arm.** The SWE-bench task distribution cannot exercise
  doc-freshness, so exposing it there only adds an unused, noisy option. The moat is validated on a
  separate doc-drift task set (out of this plan's A/B). Product surface: yes. Phase 3 retrieval surface:
  no.
- **Two registries, not one (R3-#1 — required to make the line above true in code):** there is **no
  single `AGENT_SLICE_TOOLS_V2`**. Define `V2_RETRIEVAL_TOOLS` = the five tools above (locate, orient,
  grep, related, structure) and `V2_PRODUCT_TOOLS` = `V2_RETRIEVAL_TOOLS + slice_freshness`. **Phase 3
  arms register `V2_RETRIEVAL_TOOLS` only**; the product/MCP surface uses `V2_PRODUCT_TOOLS`. A single
  six-tool list consumed by Phase 3 would smuggle freshness back into the A/B by accident.
- Hides: `slice_docs` full mode; eventually `affected-docs`/`stale-docs` wrappers.

## The forces, in one diagram (why the surface, not the retriever)

```
            EVIDENCE STATE (2026-06-06)
            ───────────────────────────
  base agent ── near ceiling ──┐
  ripgrep solves causal loc.   │   gap is MODEL/BUDGET-BOUND, not "can't find it"
  inline-collaborators FALSIFIED┘            │
                                             ▼
   ┌─────────────────────────────────────────────────────────┐
   │  Only supported lever: CONSOLIDATE the surface           │
   │  (reduce routing tax / noise), NOT add a retriever        │
   └─────────────────────────────────────────────────────────┘
                                             │
            ┌────────────────────────────────┴───────────────┐
            ▼                                                 ▼
   Phase 0.5 GATE                                   if waste NOT visible
   "do losing v1 trajectories                       → STOP. Do not build.
    actually misroute among                           Re-focus on eval trust /
    9–12 tools?"  ── waste visible? ──► build         task-class selection.
            │ yes
            ▼
   Phase 1  output-preserving consolidation (6 pass-throughs)
            │   outputs == today's commands; T1 guards no-inline-collaborators
            ▼
   Phase 2  offline adapter sanity (no model)
            ▼
   Phase 3  A/B baseline | v1 | v2   ── PRE-REGISTERED stats ──►  promote / kill
            │   gate: non-regression (recall) + routing-tax reduction
            │   + precision (powered, stratified single/multi-file)
            ▼
   Phase 4  product-surface decision (MCP / thin Rust commands)
```

## Correct Usage Patterns (sequence reference — also the Phase-3 classifiers)

These double as the **sequence classifiers** Phase 3 scores against (defined up front, per Codex, so
"clearer sequencing" is measured, not post-rationalized).

### Exact symbol / traceback / error / config key
1. `search_text(exact_term)`
2. `slice_orient(hit_file)`
3. `slice_structure(hit_file)` if ranges aren't obvious
4. `read_file(...)`
5. `slice_related(...)` only if the issue implies collaborators/callers/callees/impact

Classifier: exact-term task **starts with `search_text` by step ≤2**; `slice_locate` not the opener.

### Vague concept / unknown subsystem
1. `slice_locate(concept)` → 2. `slice_orient(candidate)` → 3. `slice_grep(candidate, term)` when a
term emerges → 4. `slice_structure(...)` → 5. `read_file(...)`. Semantic stays internal to `locate`.

Classifier: `orient` follows a `locate`/`grep` hit; `related` only after a cross-file cue.

### Cross-file behaviour / impact
1. anchor via `search_text`/`slice_locate` → 2. `slice_orient(anchor)` → 3.
`slice_related(anchor, forward)` → 4. `slice_related(anchor, reverse)` → 5. `slice_grep(scope=forward|
reverse)` (Phase 1b) → 6. read selected ranges, not the whole neighbourhood.

### Doc freshness / design drift
1. `slice_freshness(changed_files|slice)` → 2. read stale linked docs before changing code → 3. after
edits, run repo-level `slice affected-docs <changed-files> --json`.

## Implementation Plan

### Phase 0 — Contract and fixtures
Deliverables: this plan; a compact "agent navigation contract" doc for product use; the old→new mapping
table (below). Verification: contract says slice accompanies grep; every tool has one primary
responsibility; no raw lens is a peer search tool without a measured reason.

### Phase 0.5 — Trajectory-confusion precondition GATE *(NEW — D7)*
**Build nothing until this passes.** Analyse the v1 agent-loop trajectories already on disk
(`results/tools-slice*/…/*.traj.json`; each carries the full `messages` array plus
`info.tool_calls`/`info.steps`/`n_*_calls` rollups — but see **Data source** below: the rollups are
aggregate-only and insufficient for the dead-end / followed-by-read logic).

**Win/loss label, defined precisely (R2-#3)** — "split win vs loss" is not enough. Per trajectory:
- score = causal recall (`patchR`) on that single trajectory.
- **Scoring join (R3-#3) — this is net-new work, not a lookup.** Per-trajectory `patchR` is **not**
  stored in `info`; `causal_recall.py:score_instance` computes per-trajectory recall internally but only
  emits the **instance-level average** over seeds. So `0_5_trajectory_confusion.py` must compute
  per-trajectory `patchR` itself — reuse `causal_recall.py`'s `base_lines` / `reads_by_file` / `_recall`
  against the gold parquet, scoring each trajectory file individually — then **join each trajectory to
  its instance's baseline per-instance median**.
- **loss** = trajectory `patchR` below that baseline per-instance median (i.e. v1 did worse than the
  baseline arm's typical run on the same issue), OR, where no baseline run exists, below a pre-set
  threshold. Win = at or above. State the chosen rule in the readout.
- **Stratify every metric by task class** — exact-term/named-symbol vs vague-concept — because the two
  shapes route differently; a mixed bag can manufacture apparent "surface waste" that is really one
  stratum. (The classes are the same exact-term/vague split used by the Phase-3 sequence classifiers.)

**Data source (R3-#4):** the routing metrics below must be read from each trajectory's **`messages`**
array — NOT from `info.n_*_calls` (aggregate counts only), `info.steps` (tool names only), or
`info.tool_calls` (`_log_entry` collapses structured args to a single `arg`). Dead-end /
followed-by-read and full-argument logic require walking the message sequence (assistant `tool_calls`
→ the following `tool` result → the next assistant action).

Measure, on **losing** trajectories (per stratum):
- **normalized** tool-choice entropy across the 9–12 slice tools (normalized so it is not trivially a
  function of how many tools exist),
- redundant calls (same tool+arg repeated) and dead-end calls (tool result never followed by a read),
- grep displacement (does a "loud" locate tool steal calls from the breadth beat, as `slice_semantic`
  did 31→21 / 8→4?),
- read-volume, step spend per tool, and time-to-first-useful-read.

**Gate:** proceed to Phase 1 **only if** losing trajectories show measurable routing waste (high
normalized entropy / redundant / dead-end / displacement) that consolidation could plausibly remove,
**in the stratum where v1 actually loses**. If routing looks clean and losses are budget/ceiling-bound,
**STOP** and re-focus on eval trust and task-class selection (the cheaper, higher-leverage work the
falsification points to).

Deliverable: `scripts/0_5_trajectory_confusion.py` + a one-page readout. Cost: the logs exist; this is
analysis only.

### Phase 1 — Output-preserving benchmark facade *(D1)*
Implement in `slice-cli-benchmark/scripts/run_tools.py`, behind `--agent-surface v2`.

Deliverables:
- **Two tool registries (R3-#1):** `V2_RETRIEVAL_TOOLS` (five: locate, orient, grep, related,
  structure) and `V2_PRODUCT_TOOLS` (= those five + `slice_freshness`). Each schema is a **thin
  pass-through** to the existing `SLICE_BIN` command, **output identical to today's command**. No
  bundling, no scoped grep, no locate blending (those are Phase 1b). **Phase 3 registers
  `V2_RETRIEVAL_TOOLS`; never a combined six-tool list.**
- **Strict v2 schemas + arg validation (R3-#2):** the v2 tool schemas set
  `additionalProperties: false`, AND the v2 dispatch **explicitly rejects unexpected args** (returns a
  `(blocked: unknown arg …)` string). Schema omission alone is insufficient: today's schemas
  (`run_tools.py:147`) don't set `additionalProperties`, and `exec_tool` (`run_tools.py:274`) reads only
  named keys, so a stray `scope`/etc. is *silently ignored*, not rejected — which would let the model
  request an unhonored scope and have the harness quietly run owned grep instead.
- A v2 briefing (factual capability + composition; neutral; mirrors `docs/navigating.md`).
- Per-call logging that records **facade tool name + the underlying CLI command(s)** it ran.

Constraints: no Rust changes; v1 surface stays; `search_text` stays visible to the slice arm.

Verification — **routing-assertion matrix** *(D4)*, over a tmp fixture checkout (reuse the
`examples/mock-repo` shape or a staged `artifacts/checkouts` instance), following the existing
`tests/test_*.py` pure-helper style:

| Test | Asserts |
|---|---|
| **T1 (CRITICAL regression)** | `slice_orient` output does **NOT** contain the collaborator block — guards the falsified `--gate-affordances` inline-collaborators behaviour so it can't be re-added. |
| T2 `locate→find` | `slice_locate(q)` runs `find q`, nothing else. |
| T3 `orient→context` | `slice_orient(t)` runs `context t --best-effort`. |
| T4 `grep→grep --symbols` | flags map; `--symbols` defaulted on; `-i` from `ignore_case`. |
| T5 `related→deps` | `direction/depth/files` ↔ `--reverse/--transitive/--files` mapping (all combos). |
| T6 `structure` fork | file target → `outline`; slice target → `symbols`. |
| T7 `freshness` fork | path/slice → `docs`; changed-file list → `affected-docs`. |
| T8 v2-set | the v2 tool set **excludes** the hidden v1 tools (`slice_context`, `slice_for`, `slice_files`, `slice_deps`, `slice_outline`, `slice_symbols`, `slice_docs`, `slice_semantic`). |
| T9 log-schema | each call records facade name + underlying CLI command. |
| T10 dry-run | v1, v1+grep, and v2 run side by side on one checkout (existing dry-run path). |
| **T11 inventory fold (R2-#4)** | `slice_locate(intent="inventory")` (no/empty query) runs `list`; `slice_locate(q)` runs `find q`. |
| **T12 scope rejection (R2-#5 / R3-#2)** | the v2 `slice_grep` schema has no `scope` property **and** sets `additionalProperties:false`; a call passing `scope=forward` is **rejected** (`blocked: unknown arg`), not silently run as owned grep. Asserts the dispatch-level guard, since schema omission alone doesn't reject (`exec_tool` ignores extras). |
| **T13 v2 retrieval set (R3-#1)** | the Phase-3 v2 registry is `V2_RETRIEVAL_TOOLS` (5 tools) and **does not contain `slice_freshness`**; `V2_PRODUCT_TOOLS` does. |

### Phase 1b — Behavioural enrichment *(DEFERRED — gated on Phase 1 holding)*
Only after Phase 1's consolidation A/B shows non-regression. Each item here is a *behavioural* change,
A/B'd **separately** so it never confounds the consolidation measurement (Beck: structural change,
*then* behavioural change):
- `slice_locate` blending (keyword + semantic + inventory), with any routing logic;
- `slice_grep` `scope=forward|reverse|neighborhood` (net-new dep-closure file-set assembly, capped);
- `slice_orient` payload bundling (abstractions/flows/docs) beyond today's `context` output.
Each ships with the same routing/output-budget discipline and its own offline + loop check.

### Phase 2 — Offline sanity checks (no model)
`slice_locate` vs existing NL-recall corpora; `slice_grep(owned)` vs known fixture terms;
`slice_orient`/`slice_structure` output-size checks; `slice_related` truncation/ranking checks. **Gate:**
offline catches broken adapters only; it never justifies shipping — it only unblocks the loop A/B.

### Phase 3 — Agent-loop A/B *(pre-registered — D8)*

**Four arms — the control arm is mandatory (R2-#1).** Today the v1 slice arm gets **no** whole-repo
grep (`build_tools` at `run_tools.py:359` gives the slice arm `SLICE_TOOLS + base`; only baseline gets
`SEARCH_TEXT`). v2 adds `search_text` to the slice arm. Without a control, "v2 vs v1" conflates
*consolidation* with *handing the agent ripgrep* — and the evidence says ripgrep alone solves most
causal localization, so the grep addition could explain a v2 win entirely. The control isolates it:

| Arm | Surface | Isolates |
|---|---|---|
| `baseline` | `search_text` only | the floor |
| `v1` | current slice tools, **no** `search_text` | today's shipped slice arm |
| `v1+grep` (**control, NEW**) | current slice tools **+ `search_text`** | the *grep-addition* variable |
| `v2` | consolidated facade (`V2_RETRIEVAL_TOOLS`, 5 tools, **no freshness**) **+ `search_text`** | the *consolidation* variable |

Read it as: `v2` − `v1+grep` = **consolidation effect** (the question this plan asks); `v1+grep` − `v1`
= **grep-addition effect**. If you genuinely only want product-surface validation (not isolated
consolidation), you may drop `v1+grep` — but then say so explicitly and stop claiming the A/B isolates
consolidation. Default: run the control.

**Pre-registration (write before running):**
- fixed **N seeds** (choose N so the precision metric's CI separates the expected effect; reuse
  `--seeds`), **model + version pinned** (manifest `provenance` already records it),
- a **minimum effect size + CI-separation** rule for any "improves" claim,
- **task stratification**: single-file vs multi-file gold, AND exact-term vs vague-concept, reported
  separately (the multi-file shape is where the only historical signal lived),
- the **sequence classifiers** above, defined now.

Metrics: causal recall (`patchR`/test-required context), file precision, span/line coverage+precision,
read volume, grep-first-on-exact-term rate, `slice_related` displacement, and — for routing quality —
**normalized** tool-choice entropy, dead-end-call rate, repeated-call rate, time-to-first-useful-read,
and sequence-classifier compliance.

Success (ordered):
1. **Primary:** v2 does **not regress** causal recall vs `v1+grep` (the matched control) or baseline.
2. **Primary — routing quality (R2-#2):** v2 improves the *real* routing metrics — lower normalized
   entropy, lower dead-end and repeated-call rates, faster time-to-first-useful-read, higher
   sequence-classifier compliance, at equal-or-lower read volume. **"Fewer tools routed among" is NOT a
   success metric** — v2 has fewer tools by construction, so that comparison is tautological and would
   over-credit the mere rename/hide. Routing *quality*, not tool *count*, is the bar.
3. **Retained gate (D2), now powered:** v2 preserves or improves file precision vs `v1+grep`, judged by
   the pre-registered effect-size + CI rule, stratified single/multi-file. *(Eyes-open: the
   falsification says headroom here is limited; we keep the bar but refuse to read noise as a
   pass/fail.)*

Kill conditions: v2 becomes a de-facto grep replacement; v2 improves offline recall but worsens loop
context; v2 increases noisy related-file reads; v2 reduces whole-repo grep on exact-term tasks.

### Phase 4 — Product-surface decision
If v2 passes: promote the facade to the real agent integration / MCP wrapper; keep Rust CLI commands as
the substrate; add thin Rust commands matching facade names **only** if direct CLI ergonomics need them
(not merely to mirror the wrapper). **Open product question (Codex):** facade-**exclusive** vs
facade-**default-with-escape-hatch** to the rich tools — the benchmark's `v2-excludes-v1` arm tests the
exclusive case; real agents may need the escape hatch. Decide this with a product lens, not the A/B.
If v2 fails: keep the current CLI; preserve adapter lessons; re-focus on staleness/navigation evals.

## Old-to-New Mapping

| Current agent tool | Proposed facade | Phase 1 contract (output-preserving) |
|---|---|---|
| `slice_list` | `slice_locate(intent="inventory")` | inventory probe preserved via the intent fold (R2-#4), not a peer tool |
| `slice_find` | `slice_locate` | pass-through to `find` |
| `slice_semantic` | internal to `slice_locate` | **not exposed in Phase 1**; Phase 1b blend only on evidence |
| `slice_context` | `slice_orient` | pass-through to `context --best-effort` (no inline collaborators) |
| `slice_for` | hidden under `slice_orient` | hidden |
| `slice_files` | hidden under `slice_orient` | hidden |
| `slice_show` | mostly `slice_orient`; details on demand | summary parts via orient; full `show` not a peer tool |
| `slice_deps` | `slice_related` | pass-through to `deps` (direction/depth/files mapped) |
| `slice_grep` | `slice_grep` | pass-through to `grep --symbols`, owned-files only; **`scope` param omitted from the Phase 1 schema** (scoped = Phase 1b) |
| `slice_outline` | `slice_structure` | file → `outline` |
| `slice_symbols` | `slice_structure` | slice → `symbols` |
| `slice_docs` | `slice_freshness` | path/slice → `docs`; changed-list → `affected-docs` (likely out of the A/B) |

## What already exists (reuse, not rebuild)

- `run_tools.py` already has the entire adapter substrate: `exec_tool()` dispatch, `_run()` subprocess
  wrapper, `build_tools()`/`build_system()` arm assembly, per-call attribution (`_log_entry`/`_rollups`),
  hygiene gates, `MAX_TOOL_OUTPUT` truncation, `--seeds`, manifest `provenance`. The v2 facade is new
  schemas + dispatch branches over this — genuinely mechanical for orient/structure/related/freshness.
- `_collaborators_block()` and `_semantic()` already exist as the *experiments this plan supersedes*
  (the falsified gate-affordance, and the regressing peer-semantic tool). T1 exists to stop
  `_collaborators_block`'s pattern returning at the orient beat.
- Tests: `tests/test_hygiene.py`, `test_preflight.py`, `test_compare.py` — the pure-helper pattern the
  new adapter tests follow.
- Phase 0.5 consumes trajectories already written under `results/tools-slice*/…/*.traj.json`.

## NOT in scope

- **Rust CLI changes** — facade lives in the Python benchmark wrapper until proven (Phase 4 decides any
  thin Rust mirror). Deferred: avoid spending a change on an unproven surface.
- **`slice_locate` blending / LLM routing** — Phase 1b, gated. Deferred: it is net-new retrieval logic,
  exactly what the evidence says not to add first.
- **`slice_grep` forward/reverse/neighborhood scoping** — Phase 1b, gated. Deferred: net-new dep-closure
  assembly with its own perf/output surface; would confound the consolidation A/B.
- **`slice_orient` payload bundling** beyond `context` output — Phase 1b. Deferred: behavioural change;
  measured apart from consolidation.
- **Output-budget design** — Open Question #4, deliberately deferred (D3); see Failure Modes for the
  residual risk it leaves on the Phase 3 noise signal.
- **`slice_freshness` in the retrieval A/B** — likely excluded; moat tested on a separate doc-drift set.
- **Cross-repo benchmark↔slice-cli freshness linkage** — existing `TODOS.md` P3 item; orthogonal, does
  not block this.
- **MCP wrapper build** — Phase 4 only, conditional on the A/B.

## Failure modes (per new codepath)

| Codepath | Realistic production/eval failure | Test? | Handling? | Visible? |
|---|---|---|---|---|
| `slice_orient` pass-through | a future edit re-inlines collaborators (the falsified move) → coverage regresses silently | **T1 (yes)** | n/a (test-guarded) | T1 fails loud in CI |
| `slice_structure` fork | a slice id that looks file-ish (or vice-versa) routes to the wrong command | T6 | `_run` returns the binary's error verbatim | yes (tool output) |
| `slice_related` mapping | wrong flag combo (e.g. forward when reverse meant) → wrong file set, silent | T5 | none beyond `deps` semantics | **partial** — wrong-but-plausible output; T5 guards the mapping |
| any facade pass-through | `MAX_TOOL_OUTPUT=6000` chops a large `deps --transitive --files` / `symbols` mid-output, unlabeled | — | generic truncation only (Open Q#4 deferred) | **no** — silent chop |
| Phase 1b `slice_grep(forward)` | rg over a large transitive dep closure → big/slow output swamps the agent | (Phase 1b) | needs file-count cap + label | (Phase 1b) |

**Critical gap flagged (D3-residual):** the unlabeled `MAX_TOOL_OUTPUT` chop is silent *and* untested
*and* it directly muddies the Phase 3 "noisy mega-tool" kill signal — the A/B may measure the truncator,
not the design. Budgets stay an Open Question by decision (D3); this is the known cost.

## Worktree parallelization

Sequential implementation, no parallelization opportunity — Phase 0.5 → Phase 1 → tests all live in the
single `slice-cli-benchmark/scripts/run_tools.py` module (+ its `tests/`), and each phase gates the
next.

## Open Questions

1. Should `slice_grep(scope=forward|reverse)` (Phase 1b) call `slice_related` internally, or accept only
   explicit scopes from a prior `slice_related`? (Defer to Phase 1b.)
2. ~~Should `slice_freshness` be in the SWE-bench surface?~~ **RESOLVED (R2-#6): out of the Phase 3
   retrieval arms — product surface only.** Moat tested on a separate doc-drift set.
3. Should `slice_locate` return suggested `search_text` patterns, or is that too close to strategy
   coaching? (Neutrality constraint #5 says tread carefully.)
4. **What per-tool output budget** should `slice_orient`/`slice_related` enforce so they aren't noisy
   mega-tools? **Deliberately open (D3)** — but it gates the trustworthiness of the Phase 3 noise
   signal; revisit before reading Phase 3 noise metrics.

## Recommended next step

**Run Phase 0.5 first.** Analyse existing v1 trajectories for tool-routing waste. If the waste is real,
build the Phase 1 output-preserving facade (orient/structure/related first — the cleanest
pass-throughs) with the T1–T10 matrix. If the waste is not visible, stop and re-focus on eval trust and
task-class selection — the cheaper, higher-leverage work the evidence points to.

## Change log (review-driven)

- **D1** — Phase 1 reframed to *output-preserving* consolidation; net-new behaviour split into a gated
  **Phase 1b**. Isolates the consolidation variable (Beck: structure-then-behaviour).
- **D2** — file-precision kept as a Phase 3 gate (retained by choice).
- **D3** — per-tool output budgets stay an Open Question; residual noise-signal risk recorded.
- **D4** — Phase 1 verification expanded to the T1–T10 matrix incl. the **T1 critical regression** guard.
- **D5** — no new `TODOS.md` entries (budgets tracked here as Open Q#4).
- **D6** — independent Codex challenge run (outside voice).
- **D7** — added **Phase 0.5** precondition gate (trajectory-confusion analysis) before any build.
- **D8** — Phase 3 statistics **pre-registered** (seeds, model pin, effect size + CI, stratification);
  sequence classifiers defined up front.
- Codex caveats folded in: "consolidation is not literally a single variable" (call it
  output-preserving), the cost-gradient risk of hiding cheap probes, the facade-exclusive-vs-escape-hatch
  product question, and `slice_freshness` likely out of the A/B.

### Round 2 (post-v2 second-pass findings, 2026-06-06)

- **R2-#1** — Phase 3 confound: v2 adds whole-repo `search_text` to the slice arm, which v1 never had
  (`run_tools.py:359`). Added a mandatory **`v1+grep` control arm** so `v2 − v1+grep` isolates
  consolidation and `v1+grep − v1` isolates the grep addition.
- **R2-#2** — "fewer tools routed among" was a tautological success metric (v2 has fewer tools by
  construction). Replaced with routing-*quality* metrics: normalized entropy, dead-end rate,
  repeated-call rate, time-to-first-useful-read, sequence-classifier compliance.
- **R2-#3** — Phase 0.5 win/loss now precisely defined (per-instance baseline-median threshold on causal
  recall) and stratified by exact-term vs vague-concept so a mixed bag can't manufacture "surface waste."
- **R2-#4** — `slice_list` was hidden with no replacement; inventory probe preserved via
  `slice_locate(intent="inventory")` → `list` (no new tool). Test T11.
- **R2-#5** — Phase 1 `slice_grep` schema now **omits `scope`** entirely (forward/reverse/neighborhood
  are Phase 1b); prevents the model requesting unhonored scopes and measuring schema friction as noise.
  Test T12.
- **R2-#6** — `slice_freshness` resolved **out of the Phase 3 retrieval arms** (product surface only);
  Open Q#2 closed.

### Round 3 (implementation-fidelity findings, 2026-06-06)

Code-grounded against the live harness — each makes an R2 spec promise actually true in code.

- **R3-#1** — Contradiction: a single six-tool `AGENT_SLICE_TOOLS_V2` would include `slice_freshness`,
  re-smuggling it into Phase 3 despite R2-#6. Split into **`V2_RETRIEVAL_TOOLS` (5)** and
  **`V2_PRODUCT_TOOLS` (= +freshness)`**; Phase 3 registers the retrieval set only. Test T13.
- **R3-#2** — T12's "unknown scope rejected" is unenforceable by schema omission: schemas at
  `run_tools.py:147` set no `additionalProperties:false`, and `exec_tool` (`run_tools.py:274`) ignores
  unused args. Require `additionalProperties:false` **and** explicit dispatch-level arg rejection. T12
  rewritten to assert the rejection, not just the omission.
- **R3-#3** — Phase 0.5 scoring join: per-trajectory `patchR` isn't stored;
  `causal_recall.py:score_instance` emits only the instance-level average. `0_5_trajectory_confusion.py`
  must compute per-trajectory `patchR` (reusing `base_lines`/`reads_by_file`/`_recall`) and join to
  baseline per-instance medians.
- **R3-#4** — Phase 0.5 data source: rollups/`info.steps`/`info.tool_calls` are aggregate or
  arg-collapsed (`_log_entry`, `run_tools.py:527`); dead-end / followed-by-read logic must read the
  trajectory `messages` array.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | issues_open (via autoplan, a7d2dc4) | prior autoplan pass |
| Codex Review | `/codex review` | Independent 2nd opinion | 1 | issues_found | 13 problems; 3 → cross-model tension |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 (+R2,+R3) | issues_open | 7 round-1 + 6 round-2 + 4 round-3 (impl-fidelity) findings folded in; 1 critical gap (silent output chop) deferred |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | n/a (CLI/benchmark, no UI) |
| DX Review | `/plan-devex-review` | Developer experience gaps | 1 | issues_open (via autoplan, score 5/10) | prior autoplan pass |

- **CODEX:** independent challenge run (gpt-5.5, high). Reinforced the limited-headroom + budget
  concerns; surfaced one new high-value move (Phase 0.5 precondition gate) and one new gate-power
  concern (pre-register Phase 3 stats). Both adopted (D7, D8).
- **CROSS-MODEL:** 3 tension points raised, all resolved by the user — Phase 0.5 gate added (D7),
  Phase 3 stats pre-registered while keeping the precision gate (D8), `slice_freshness` lean-out-of-A/B
  noted (Open Q#2). No unresolved cross-model disagreement.
- **ROUND 2:** 6 further findings (R2-#1…#6) folded in — the material one is **R2-#1**, the grep-addition
  confound, now fixed with a mandatory `v1+grep` control arm.
- **ROUND 3:** 4 implementation-fidelity findings (R3-#1…#4) folded in — each makes an R2 promise true
  in the live harness (two tool registries, real scope rejection, the per-trajectory scoring join, and
  reading `messages` not rollups).
- **UNRESOLVED:** 0 decisions left open (D1–D8 answered; R2-#1…#6 applied; Open Q#2 closed).
- **VERDICT:** ENG reviewed — plan revised to v2 (+R2) and ready to implement, **starting with Phase 0.5
  as a hard gate**, and Phase 3 **must run the `v1+grep` control arm**. 1 critical gap (silent
  `MAX_TOOL_OUTPUT` chop) is knowingly deferred (D3) and recorded in Failure Modes; not ship-blocking for
  the benchmark-internal work but revisit before trusting Phase 3 noise metrics.
