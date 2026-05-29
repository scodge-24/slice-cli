# Rust baseline benchmarks

Generated: 2026-05-29

Repo: `examples/mock-repo`
Repeat: 30
Show slice: `auth-service`
Path arg: `src/auth/middleware.py`

| Command | Mean ms | Min ms | Max ms | P95 ms |
|---------|---------|--------|--------|--------|
| `python list --json` | 239.22 | 170.80 | 361.41 | 320.11 |
| `rust list --json` | 3.17 | 2.27 | 4.91 | 4.43 |
| `python show auth-service --json` | 218.10 | 168.77 | 272.25 | 268.07 |
| `rust show auth-service --json` | 3.23 | 2.30 | 5.30 | 4.07 |
| `python for src/auth/middleware.py --json` | 216.08 | 186.08 | 253.78 | 251.62 |
| `rust for src/auth/middleware.py --json` | 3.10 | 2.06 | 4.66 | 4.47 |
| `python context src/auth/middleware.py --json` | 229.57 | 176.79 | 273.17 | 262.37 |
| `rust context src/auth/middleware.py --json` | 3.29 | 2.49 | 4.13 | 3.99 |
| `python affected-docs src/auth/middleware.py --json` | 238.74 | 188.42 | 313.09 | 267.06 |
| `rust affected-docs src/auth/middleware.py --json` | 3.23 | 2.56 | 4.73 | 4.19 |
