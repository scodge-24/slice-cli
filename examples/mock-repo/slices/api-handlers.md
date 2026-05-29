---
slice_id: api-handlers
description: API endpoint handlers and routing
loc: 30
files:
  - src/api/handlers.py
  - src/api/routes.py
abstractions:
  - "get_user — fetch one user"
  - "list_users — fetch all users"
  - "match_route — resolve a path to a handler"
dependencies:
  - auth-service
  - data-model
---

Request handlers and URL routing. All authenticated endpoints use `require_auth`
from the auth-service slice. Route resolution via `match_route`.

This slice has no `## Verification` section yet, so `slice check --require-verification`
reports its abstractions as a V-model coverage gap — the contrast with the fully-linked
auth-service slice.
