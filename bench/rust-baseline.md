# Rust baseline benchmarks

Generated: 2026-05-29

Repo: `examples/mock-repo`
Repeat: 30
Show slice: `auth-service`
Path arg: `src/auth/middleware.py`

| Command | Mean ms | Min ms | Max ms | P95 ms |
|---------|---------|--------|--------|--------|
| `python list --json` | 167.75 | 136.90 | 202.72 | 200.25 |
| `rust list --json` | 2.80 | 2.45 | 3.44 | 3.33 |
| `python show auth-service --json` | 166.61 | 140.75 | 207.73 | 187.44 |
| `rust show auth-service --json` | 2.76 | 2.26 | 3.30 | 3.27 |
| `python for src/auth/middleware.py --json` | 157.70 | 133.24 | 180.85 | 174.82 |
| `rust for src/auth/middleware.py --json` | 2.52 | 2.16 | 3.58 | 3.18 |
| `python context src/auth/middleware.py --json` | 168.42 | 144.09 | 210.86 | 195.75 |
| `rust context src/auth/middleware.py --json` | 2.43 | 2.12 | 3.20 | 3.03 |
| `python affected-docs src/auth/middleware.py --json` | 169.94 | 147.34 | 193.53 | 190.57 |
| `rust affected-docs src/auth/middleware.py --json` | 2.95 | 2.33 | 3.89 | 3.54 |
