---
slice_id: api-handlers
description: API endpoint handlers and routing
loc: 30
files:
  - src/api/handlers.py
  - src/api/routes.py
dependencies:
  - auth-service
  - data-model
---

Request handlers and URL routing. All authenticated endpoints use `require_auth`
from the auth-service slice. Route resolution via `match_route`.
