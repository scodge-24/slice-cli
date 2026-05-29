# Rust baseline benchmarks

Generated: 2026-05-29

Repo: `examples/mock-repo`
Repeat: 30
Show slice: `auth-service`
Path arg: `src/auth/middleware.py`

| Command | Mean ms | Min ms | Max ms | P95 ms |
|---------|---------|--------|--------|--------|
| `python list --json` | 168.74 | 150.55 | 195.72 | 191.13 |
| `rust list --json` | 3.02 | 2.32 | 4.74 | 3.66 |
| `python show auth-service --json` | 160.05 | 142.91 | 183.35 | 177.47 |
| `rust show auth-service --json` | 3.27 | 2.42 | 4.10 | 3.92 |
| `python for src/auth/middleware.py --json` | 164.13 | 149.16 | 191.64 | 183.58 |
| `rust for src/auth/middleware.py --json` | 2.78 | 2.12 | 4.82 | 3.16 |
| `python context src/auth/middleware.py --json` | 176.03 | 153.70 | 206.67 | 203.01 |
| `rust context src/auth/middleware.py --json` | 3.06 | 2.41 | 4.73 | 3.83 |
| `python affected-docs src/auth/middleware.py --json` | 177.80 | 154.65 | 218.10 | 206.06 |
| `rust affected-docs src/auth/middleware.py --json` | 2.69 | 2.15 | 3.40 | 3.19 |
