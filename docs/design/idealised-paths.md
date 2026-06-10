# Idealised paths — and what detracts the agent from them

> **⚠ PREMISE FALSIFIED (2026-06-06) — keep this doc as a record of the reasoning, not a live model.**
> The core claim below — coverage is lost at one gate because the agent *can't see* collaborators, so
> the fix is to make them visible at the ORIENT beat — was tested directly (`--gate-affordances`
> inlined the collaborator files into `slice_context`) and **failed**: xarray coverage went 0.90 →
> 0.70, not up. The falsifying fact is a count, not a noisy delta: **base already reads the gold
> collaborator `utils.py` 4/5 seeds** — it was never invisible. The inline list just added noise that
> displaced a working process. What actually held: base (slice+stage2+thinking) is near its ceiling,
> and *every* retrieval-surface addition (AST, semantic ×2, inline-collaborators) is neutral-to-harmful
> → the only supported direction is **consolidation** (fewer/cleaner tools), and the residual gap looks
> model/budget-bound. See `FINDINGS.md` §"gate-affordance test FALSIFIES…". Detractors 4 (advisory
> handoffs) and 5 (overlap/displacement) survive as real; detractor 2 (invisibility) does not — the gap
> was never invisible to base.

Companion to [`slice-cli-feature-map.md`](slice-cli-feature-map.md). The point of this doc is **not** a
prescriptive "after tool 1 call tool 2" playbook — the ideal path is obvious and the agent could
reconstruct it. The point is the **force diagram**: why, given the obvious path, the agent reliably
drifts off it. Every weak spot we measured turned out to be a *path deviation*, not a missing tool, so
the leverage is in understanding the forces, not adding steps. Evidence refs point at
[`FINDINGS.md`](../../../slice-cli-benchmark/FINDINGS.md) (2026-06-06 snapshot line numbers).

## The ideal path, in one line (the reference, not the focus)

Every query shape reduces to the same four beats; only the first varies:

> **locate → orient → EXPAND-to-collaborators → read → done**

`locate` is `grep`/`find`/`context`/`for` depending on whether the target is named by symbol, file,
behaviour, or traceback. `orient` is `context`/`show`. **`EXPAND` is the constant beat — `deps --files`
or the `context` blast-radius — and it is the one the agent skips.** On single-file gold its absence is
invisible (flask: coverage 1.0); on multi-file gold its absence *is* the failure (xarray: 0.60, found
the primary, never walked to the collaborator). So the whole problem is: **why does the agent skip
EXPAND (and, before that, over-invest in locate)?** The rest of this doc is the answer.

## The detractors

The forces below all push the *same direction* — toward `locate → read → done` and away from EXPAND.
That alignment is why the failure is so consistent: there is no countervailing force in the current
surface.

### 1. Locate feels like progress; EXPAND feels like a detour

`grep`/`find` return immediate `file:line` hits — visible, gratifying "I found something" signal. Each
hit invites a read; each read suggests another search. The agent settles into a **locate loop**
(`grep→read→grep→read`) that always feels productive because it always returns *more matches*. EXPAND,
by contrast, returns *files you haven't asked about yet* — speculative, no guarantee of relevance, no
"found it" hit. A locally-greedy policy (advance toward the answer at every step) systematically
prefers the beat with the immediate payoff. **Evidence:** the slice arm collapsed into `slice_grep→
read_file` for ~77% of calls in the original routing diagnosis; `slice_grep` is still 30–50% of calls,
`slice_deps` ~once per several trajectories. `FINDINGS` §"routing experiment" (L17), §"Improvement
directions" (L787).

### 2. Incompleteness is invisible — the agent stops at *plausible*, not *complete*

When the agent has located and read the symbol the issue names, **nothing in the trajectory tells it
the job is unfinished.** There is no "you have covered 1 of 3 gold files" signal — because the agent
can't see the gold. So it stops at the first internally-coherent answer. This is the deepest
detractor: the agent isn't lazy or wrong, it is *optimising a different objective* (plausible fix
location) than the benchmark scores (complete context). The multi-file shape of the gold is
unobservable from inside the loop, so the collaborators are not "skipped" so much as *never entered the
search space*. **Evidence:** xarray (2-file gold) regresses exactly where the collaborator is unnamed;
flask (1-file gold) never regresses regardless of config. `FINDINGS` §"agent-loop test" (L552).

### 3. The issue text anchors attention on what is *named*

The query frames the agent. xarray's issue names `Variable.__setitem__`; the fix also touches
`utils.py`, which the issue never mentions. The agent treats "find and read the named symbol" as the
task and the unnamed collaborator is off-radar — and **no tool output counteracts that bias.** The
named thing gets all the attention budget; the structural relationship that would surface the unnamed
collaborator (the dependency edge) is the very beat the agent skips (detractor #1). Named-over-attended
+ EXPAND-skipped compound: the one mechanism that could rescue the unnamed file is the one the agent
avoids.

### 4. Beat handoffs are advisory pointers, not inline payloads — every boundary is an exit ramp

`context` emits a `blast-radius:` *one-liner that names the next command* rather than carrying the
collaborator files inline. Acting on it is a multi-step chain: see pointer → decide to act → call
`deps` → parse → decide which files → read. **Every one of those micro-decisions is a chance to drop
back into the locate loop.** The friction of crossing a beat boundary is itself a detractor: the path
of least resistance at any boundary is "search again," because that needs no new tool and no
parsing. The affordance exists but it is *advisory*, so it loses to the *frictionless* default.

### 5. Surface overlap converts "next beat" into "which of N redundant tools" — and a loud tool steals the beat

Four tools answer "locate" (`find`, `find --semantic`, `grep`, baseline `search_text`). The agent
doesn't just pick badly; the choice itself is a **routing tax** that re-fires at every step, and the
default resolution is the most familiar (`grep`) or the newest/loudest. Worse, adding a confident new
locator actively *displaces* the breadth beat: the semantic tool didn't add coverage, it **stole calls
from `slice_context` (31→21) and `slice_deps` (8→4)** — it made the over-invested beat louder and the
skipped beat quieter. **Evidence:** `FINDINGS` §"agent-loop test" (L552) and the breadth-advertising
iteration (L591, which recovered 2/3 instances precisely by re-routing attention to the skipped beat).

### 6. Confident near-misses outcompete broad-but-uncertain signal

A ranked semantic list with high cosine scores *looks* authoritative; the top hits are often
plausible-but-wrong (the non-gold `indexing.py:632 __setitem__` outranked the gold `variable.py:545`).
The agent trusts the confident-narrow signal over the uncertain-broad one (a dependency edge has no
score, no snippet, no "this is the answer" affordance). **Precision-of-presentation beats
relevance-of-breadth** — the better-*looking* signal wins the read budget even when the better signal
is the unscored structural one. `FINDINGS` §"agent-loop test" (L552).

### 7. Budget + convergence prior cut exploration, and EXPAND is the first casualty

With a finite step budget and a model prior toward converging on an answer, the agent trims the beat it
values least. EXPAND is the "extra" beat (the answer feels already-found), so it is trimmed first.
**Evidence:** the semantic arm *converged more* (more `submitted`, fewer `max_steps`) while covering
*less* — confidence-to-stop arrived before coverage did. `FINDINGS` §"agent-loop test" (L552).

## Synthesis — why it's so consistent, and what it implies

The deviation is reliable because **every force points the same way**: the agent's local-greedy
policy (1, 7), the invisibility of incompleteness (2), the query's framing bias (3), the advisory (not
inline) handoffs (4), the overlapping surface (5), and the seduction of confident-narrow hits (6) *all*
favour `locate → read → done` and *all* penalise EXPAND. There is no force in the current surface
pushing back. So the agent isn't malfunctioning — it is correctly following the gradient the surface
presents, and the surface's gradient points away from collaborator coverage.

That reframes the design problem. It is **not** "add a better retriever" (that strengthens the
already-dominant locate beat — see semantic). It is **flip the gradient on the two beats that lose**:

- **Make incompleteness visible (counter to 2/3/7).** The agent can't seek what it can't see is
  missing. A signal that surfaces the *unentered* search space — "this slice has N collaborator files
  you have not read" — turns a silent gap into an observable one. This is the highest-leverage missing
  affordance; everything else is downstream of the agent not knowing it's at 60%.
- **Make the EXPAND beat frictionless and inline (counter to 4/1).** Carry the collaborator *files*
  inside the locate/orient output instead of pointing at the command that would fetch them, so
  crossing the beat boundary costs zero new decisions. The breadth-advertising iteration was a first
  cut at this and recovered 2/3 instances; it failed on xarray because the pointer was still advisory
  (a `deps(...)` suggestion at the end of the list), not the files themselves.
- **Collapse the locate surface (counter to 5/6).** One locate beat → one locate tool with the
  modality chosen internally; removes the routing tax and denies any single modality the chance to
  steal the beat by looking loud.

In short: the lever is not the tools' *capabilities* but the *gradient their outputs create*. Today
each tool describes itself; the fix is for each to (a) make the next beat the path of least resistance
and (b) make the gap the agent can't see visible. That is principle (P) — affordance/ACI — stated as a
force-balancing problem rather than a feature list. See [`agentic-search-incorporation-plan.md`](../../design/agentic-search-incorporation-plan.md)
§8 (L431) and the feature-map's *Feature overlap & purpose* section.
