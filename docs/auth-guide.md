---
doc_id: auth-guide
title: Authentication Guide
tags: [auth, middleware, security]
---

# Authentication Guide

All API endpoints use JWT-based authentication via `verify_token()` in
`src/auth/middleware.py`. The `require_auth` decorator extracts the Bearer
token from the Authorization header and attaches decoded claims to `request.user`.

## Session Management

Sessions are managed in-memory via `src/auth/sessions.py`. Call
`create_session(user_id)` to issue a session ID, and `destroy_session(sid)`
to invalidate it.
