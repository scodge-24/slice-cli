# Rust baseline benchmarks

Generated: 2026-05-29

Repo: `examples/mock-repo`
Repeat: 30
Show slice: `auth-service`
Path arg: `src/auth/middleware.py`

| Command | Mean ms | Min ms | Max ms | P95 ms |
|---------|---------|--------|--------|--------|
| `python list --json` | 178.61 | 152.40 | 219.10 | 207.92 |
| `rust list --json` | 3.04 | 2.35 | 4.89 | 4.17 |
| `python show auth-service --json` | 173.61 | 150.71 | 215.42 | 195.16 |
| `rust show auth-service --json` | 3.18 | 2.42 | 4.39 | 4.05 |
| `python for src/auth/middleware.py --json` | 166.68 | 145.88 | 196.79 | 179.04 |
| `rust for src/auth/middleware.py --json` | 2.96 | 2.35 | 4.19 | 4.12 |
| `python context src/auth/middleware.py --json` | 186.84 | 161.95 | 226.59 | 220.81 |
| `rust context src/auth/middleware.py --json` | 3.09 | 2.38 | 4.26 | 4.16 |
| `python affected-docs src/auth/middleware.py --json` | 237.67 | 159.94 | 424.43 | 394.50 |
| `rust affected-docs src/auth/middleware.py --json` | 3.09 | 2.37 | 4.50 | 4.26 |
