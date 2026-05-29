#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPEAT="${REPEAT:-30}"
REPO="${REPO:-examples/mock-repo}"
SHOW_SLICE="${SHOW_SLICE:-auth-service}"
PATH_ARG="${PATH_ARG:-src/auth/middleware.py}"
OUT="${OUT:-$ROOT/bench/rust-baseline.md}"
RUST_BIN="$ROOT/rust/slice-rs/target/release/slice-rs"

cd "$ROOT"
cargo build --release --manifest-path rust/slice-rs/Cargo.toml >/dev/null

run_stats() {
  local label="$1"
  shift
  python3 - "$REPEAT" "$label" "$@" <<'PY'
import math
import statistics
import subprocess
import sys
import time

repeat = int(sys.argv[1])
label = sys.argv[2]
cmd = sys.argv[3:]
samples = []
for _ in range(repeat):
    start = time.perf_counter_ns()
    subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
    samples.append((time.perf_counter_ns() - start) / 1_000_000)
samples.sort()
p95_index = min(len(samples) - 1, math.ceil(len(samples) * 0.95) - 1)
print(
    f"| `{label}` | {statistics.mean(samples):.2f} | {min(samples):.2f} | "
    f"{max(samples):.2f} | {samples[p95_index]:.2f} |"
)
PY
}

{
  cat <<EOF
# Rust baseline benchmarks

Generated: $(date +%Y-%m-%d)

Repo: \`$REPO\`
Repeat: $REPEAT
Show slice: \`$SHOW_SLICE\`
Path arg: \`$PATH_ARG\`

| Command | Mean ms | Min ms | Max ms | P95 ms |
|---------|---------|--------|--------|--------|
EOF
  run_stats "python list --json" python3 -m slice_cli --repo "$REPO" list --json
  run_stats "rust list --json" "$RUST_BIN" --repo "$REPO" list --json
  run_stats "python show $SHOW_SLICE --json" python3 -m slice_cli --repo "$REPO" show "$SHOW_SLICE" --json
  run_stats "rust show $SHOW_SLICE --json" "$RUST_BIN" --repo "$REPO" show "$SHOW_SLICE" --json
  run_stats "python for $PATH_ARG --json" python3 -m slice_cli --repo "$REPO" for "$PATH_ARG" --json
  run_stats "rust for $PATH_ARG --json" "$RUST_BIN" --repo "$REPO" for "$PATH_ARG" --json
  run_stats "python context $PATH_ARG --json" python3 -m slice_cli --repo "$REPO" context "$PATH_ARG" --json
  run_stats "rust context $PATH_ARG --json" "$RUST_BIN" --repo "$REPO" context "$PATH_ARG" --json
  run_stats "python affected-docs $PATH_ARG --json" python3 -m slice_cli --repo "$REPO" affected-docs "$PATH_ARG" --json
  run_stats "rust affected-docs $PATH_ARG --json" "$RUST_BIN" --repo "$REPO" affected-docs "$PATH_ARG" --json
} > "$OUT"

cat "$OUT"
