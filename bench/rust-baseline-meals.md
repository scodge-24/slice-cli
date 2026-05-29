# Rust baseline benchmarks

Generated: 2026-05-29

Repo: `/home/scodge/dev/meals`
Repeat: 20
Show slice: `backend-meal-logs`
Path arg: `apps/backend/src/services/meal-logs.ts`

| Command | Mean ms | Min ms | Max ms | P95 ms |
|---------|---------|--------|--------|--------|
| `python list --json` | 278.47 | 224.11 | 339.87 | 327.12 |
| `rust list --json` | 3.87 | 2.86 | 6.01 | 4.67 |
| `python show backend-meal-logs --json` | 263.28 | 222.07 | 307.89 | 306.42 |
| `rust show backend-meal-logs --json` | 4.39 | 3.01 | 12.08 | 6.24 |
| `python for apps/backend/src/services/meal-logs.ts --json` | 299.82 | 245.46 | 355.27 | 344.23 |
| `rust for apps/backend/src/services/meal-logs.ts --json` | 3.97 | 2.95 | 4.99 | 4.89 |
| `python context apps/backend/src/services/meal-logs.ts --json` | 299.32 | 249.97 | 337.18 | 326.66 |
| `rust context apps/backend/src/services/meal-logs.ts --json` | 4.11 | 3.10 | 6.16 | 5.90 |
| `python affected-docs apps/backend/src/services/meal-logs.ts --json` | 297.69 | 246.75 | 341.93 | 341.19 |
| `rust affected-docs apps/backend/src/services/meal-logs.ts --json` | 3.98 | 2.87 | 6.41 | 5.61 |
