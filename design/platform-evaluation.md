# Platform Evaluation: Python vs a Rust/TS port

## Why this doc exists

slice-cli works today in Python and is fully type-hinted. The open question is
whether it should be ported to a faster, more strongly-typed language (Rust or
TypeScript) as it scales to larger repos. This doc turns that instinct into a
**measured, trigger-based decision** rather than a hunch. Re-run `bench/run.py`
and update the numbers; port only when a trigger fires.

## Method

`bench/run.py` synthesizes a throwaway git repo with N slices (each owning K
source files) and N tracked docs, stamps them, then times the hot commands via
the real CLI (subprocess — includes interpreter startup, ~the user's experience).

```bash
python bench/run.py --slices 200 --files-per-slice 3 --repeat 3
```

## Results (baseline)

200 slices x 3 files (600 source files, 200 tracked docs), best of 3, on the
author's dev machine:

| command                  | time    |
|--------------------------|---------|
| `check`                  | ~750 ms |
| `stale-docs`             | ~520 ms |
| `affected-docs` (1 file) | ~610 ms |
| `list`                   | ~1080 ms |

Notes:
- A fixed ~80-120 ms of every run is Python interpreter + import startup.
- `list` is the slowest because it computes a doc-count per slice (O(slices x
  docs)); it's the first candidate for optimization if it ever matters.
- The real work is git subprocess calls, file I/O, and content hashing — not
  Python compute. A language port speeds up the compute fraction, which is small.

## Port triggers (decide on data, not vibes)

Consider a Rust/TS port **only if one of these fires** and in-Python optimization
(caching, avoiding repeated git calls, lazy doc-count) has already been tried:

1. **Latency:** `check` or `affected-docs` exceeds ~2 s at a realistic repo size
   for the target user (re-benchmark at that size first).
2. **Scale:** real repos routinely exceed ~2000 slices and the O(slices x docs)
   paths dominate even after caching.
3. **Distribution:** a zero-dependency single binary becomes a hard requirement
   (no Python on target machines) that `pipx`/`pyinstaller` can't satisfy.
4. **Agent-safety:** stronger compile-time guarantees become necessary beyond
   what `pyright --strict` in CI already provides.

If none fire, stay on Python: it ships today, the hot path is I/O-bound, and a
rewrite would discard a working, tested implementation.

## In-Python levers to pull first

- Cache `load_slice_docs` / `load_doc_manifest` within a single invocation.
- Make `list`'s doc-count lazy or precomputed.
- Batch or memoize git calls in `check_doc_drift`.
