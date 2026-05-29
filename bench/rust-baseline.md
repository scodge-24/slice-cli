# Rust baseline benchmarks

Generated: 2026-05-29

Repo: `examples/mock-repo`
Repeat: 30
Show slice: `auth-service`
Path arg: `src/auth/middleware.py`

| Command | Mean ms | Min ms | Max ms | P95 ms |
|---------|---------|--------|--------|--------|
| `python list --json` | 170.13 | 141.88 | 195.83 | 195.32 |
| `rust list --json` | 2.81 | 2.43 | 3.90 | 3.79 |
| `python show auth-service --json` | 170.40 | 152.57 | 192.41 | 189.32 |
| `rust show auth-service --json` | 3.27 | 2.37 | 4.73 | 4.13 |
| `python for src/auth/middleware.py --json` | 221.07 | 141.24 | 516.21 | 427.93 |
| `rust for src/auth/middleware.py --json` | 3.65 | 3.20 | 4.70 | 4.33 |
| `python context src/auth/middleware.py --json` | 191.85 | 160.67 | 384.54 | 237.20 |
| `rust context src/auth/middleware.py --json` | 2.96 | 2.63 | 4.06 | 3.89 |
| `python affected-docs src/auth/middleware.py --json` | 180.59 | 154.98 | 225.35 | 211.29 |
| `rust affected-docs src/auth/middleware.py --json` | 2.96 | 2.32 | 4.32 | 4.08 |
