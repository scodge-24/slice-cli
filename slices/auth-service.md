---
slice_id: auth-service
description: Authentication and session management
loc: 45
files:
  - src/auth/middleware.py
  - src/auth/sessions.py
dependencies: []
---

Handles JWT verification, the `require_auth` decorator, and in-memory session
lifecycle. Entry points are `verify_token` and `require_auth` in middleware,
plus `create_session`/`get_session`/`destroy_session` in sessions.
