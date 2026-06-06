# Navigating a codebase with `slice`

A human-facing companion to [`agent-workflows.md`](agent-workflows.md). Once a repo has
slices (`/slice-codebase` generates them), `slice` answers the questions you actually ask
when you land in unfamiliar code: *what owns this file, what will I break if I change it,
where does this concept live, how do requests flow?* Doc-staleness tracking is one feature
among these, not the headline.

All examples run against the bundled demo with `--repo examples/mock-repo`; drop that flag
in a real repo.

## Orient on a file

Starting from a path you're about to edit:

```bash
slice context src/auth/middleware.py
```

You get the owning slice, its description, the files it covers, its dependencies, and the
slice's durable notes — `## Runtime Flows` (call chains), `## System Behavior`,
`## Invariants`, `## Verification`. Add `--json` for machine output, which additionally
carries the linked docs and their stale/current status.

Just need the owning slice id?

```bash
slice for src/auth/middleware.py
```

## Explore the structure

```bash
slice list                       # every slice: id, description, LoC
slice show auth-service          # one slice's metadata + linked docs
slice show auth-service --system        # full system notes
slice show auth-service --call-stacks   # just the runtime flows
slice show auth-service --verification  # just the V-model links
```

## Blast radius before you change something

The question that prevents most regressions — *who depends on this?*

```bash
slice deps auth-service                       # what this slice depends on (direct)
slice deps auth-service --transitive          # …and transitively
slice deps auth-service --reverse             # who depends on this slice (direct)
slice deps auth-service --reverse --transitive   # the full upstream blast radius
slice deps auth-service --reverse --transitive --files  # …as concrete files to read
```

Run the reverse-transitive form before editing a low-level slice: it lists every slice
that would be affected, directly or through intermediaries.

Add `--files` to turn that radius into the concrete collaborator files you'd actually open —
each tagged with the slice that owns it, nearest dependents first, deduplicated — so you read
them directly instead of re-deriving paths from slice ids. `slice context <file>` also prints a
one-line `blast-radius:` summary pointing at this command, so the radius is visible without
asking for it.

## Find a concept

```bash
slice find idempotency      # which slices mention a concept/abstraction
```

`find` searches slice descriptions, abstractions, and bodies, and tags each hit with where
it matched (`[abstractions]`, `[body]`, …).

### Semantic search (opt-in, `semantic` build feature)

When you can't name the symbol, describe the behaviour in natural language and rank by
meaning instead of keyword. Built only with the `semantic` feature; needs `OPENROUTER_API_KEY`.

```bash
slice semantic-index --units code      # build the embedding index once (regenerable, slice-owned)
slice find "reject an empty blueprint name" --semantic --units code
```

`--units cards` embeds each slice's description + abstractions; `--units code` embeds the
source symbols themselves, so every hit carries the matching symbol's `file:line`. The vector
score is only a candidate generator — the final ranking is deterministic (owning slice,
reverse-dependency distance, freshness), and a hit whose owning slice has drifted since the
index was built is flagged stale rather than silently returned.

To grep the actual source files owned by a slice:

```bash
slice grep auth-service "verify_token"      # ripgrep, scoped to that slice's files
slice grep auth-service -i "token"          # case-insensitive
```

`slice grep` needs `rg` (ripgrep) on PATH.

## Browse interactively (the TUI)

```bash
slice browse           # fuzzy-pick a slice, preview it inline (needs fzf >= 0.30)
slice browse -q auth   # start with an initial query
```

`enter` shows the selected slice. The preview pane switches lenses without leaving the
picker:

| Key | Lens |
|-----|------|
| `ctrl-o` | overview (the slice card) |
| `ctrl-r` | runtime call-stacks |
| `ctrl-d` | verification links |
| `ctrl-t` | reverse deps (blast radius) |

For scripting, `--print` emits the chosen id instead of showing it:

```bash
id=$(slice browse --print) && slice show "$id"   # && guards against cancel
```

## Doc staleness (the other half)

When you change code, find the docs that may now be wrong, and re-bless them once updated:

```bash
slice affected-docs src/auth/middleware.py   # docs a change may have made stale
slice stale-docs                             # everything currently stale (exit 1 if any)
slice stamp auth-guide                        # mark a doc verified against current code
```

`slice stale-docs` exits non-zero when anything is stale, so it doubles as a CI gate.
