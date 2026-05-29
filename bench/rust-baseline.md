# Rust baseline benchmarks

Generated: 2026-05-29

Repo: `examples/mock-repo`
Repeat: 30
Show slice: `auth-service`
Path arg: `src/auth/middleware.py`

| Command | Mean ms | Min ms | Max ms | P95 ms |
|---------|---------|--------|--------|--------|
| `python list --json` | 158.27 | 141.88 | 186.64 | 178.42 |
| `rust list --json` | 2.88 | 2.09 | 4.06 | 3.78 |
| `python show auth-service --json` | 155.22 | 144.38 | 171.58 | 171.06 |
| `rust show auth-service --json` | 2.86 | 2.40 | 3.74 | 3.42 |
| `python for src/auth/middleware.py --json` | 154.83 | 139.62 | 173.34 | 172.58 |
| `rust for src/auth/middleware.py --json` | 2.99 | 2.46 | 4.50 | 3.99 |
| `python context src/auth/middleware.py --json` | 166.12 | 153.49 | 201.35 | 187.00 |
| `rust context src/auth/middleware.py --json` | 2.95 | 2.34 | 3.51 | 3.50 |
| `python affected-docs src/auth/middleware.py --json` | 163.51 | 152.58 | 180.17 | 177.39 |
| `rust affected-docs src/auth/middleware.py --json` | 2.71 | 2.12 | 3.68 | 3.47 |
