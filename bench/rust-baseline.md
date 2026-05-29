# Rust baseline benchmarks

Generated: 2026-05-29

Repo: `examples/mock-repo`
Repeat: 30
Show slice: `auth-service`
Path arg: `src/auth/middleware.py`

| Command | Mean ms | Min ms | Max ms | P95 ms |
|---------|---------|--------|--------|--------|
| `python list --json` | 210.26 | 139.65 | 394.83 | 387.07 |
| `rust list --json` | 2.76 | 2.19 | 3.41 | 3.27 |
| `python show auth-service --json` | 152.48 | 141.30 | 172.90 | 168.97 |
| `rust show auth-service --json` | 2.74 | 2.33 | 3.24 | 3.24 |
| `python for src/auth/middleware.py --json` | 156.22 | 143.68 | 174.85 | 171.96 |
| `rust for src/auth/middleware.py --json` | 2.83 | 2.45 | 3.48 | 3.31 |
| `python context src/auth/middleware.py --json` | 169.21 | 151.03 | 200.30 | 196.70 |
| `rust context src/auth/middleware.py --json` | 3.00 | 2.47 | 4.83 | 3.64 |
| `python affected-docs src/auth/middleware.py --json` | 166.27 | 147.81 | 184.31 | 183.14 |
| `rust affected-docs src/auth/middleware.py --json` | 2.62 | 2.11 | 4.07 | 3.34 |
