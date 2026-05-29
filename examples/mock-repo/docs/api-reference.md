---
doc_id: api-reference
title: API Reference
tags: [api, routes, handlers]
---

# API Reference

## Endpoints

| Method | Path | Handler | Auth |
|--------|------|---------|------|
| GET | /users/:id | get_user | Yes |
| GET | /users | list_users | Yes |
| GET | /health | health_check | No |

## Routing

Routes are defined in `src/api/routes.py`. The `match_route(path)` function
resolves a URL path to its handler function.
