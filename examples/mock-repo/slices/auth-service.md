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

## System Behavior

Every protected request passes through `require_auth`, which calls
`verify_token` and then resolves the session. Unauthenticated requests are
rejected before the handler runs.

## Invariants

- A token is valid only if unexpired AND its session still exists.
- Sessions live in memory; a process restart drops all sessions.

## Runtime Flows

request -> require_auth -> verify_token -> get_session -> handler

## Verification

Exercise `verify_token` with a fresh, an expired, and a tampered token; confirm
`destroy_session` makes a previously-valid token fail.

## Update Triggers

Update this doc when token expiry handling, the session store, or the
`require_auth` contract changes.
