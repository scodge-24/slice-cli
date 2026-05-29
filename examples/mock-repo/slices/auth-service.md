---
slice_id: auth-service
description: Authentication and session management
loc: 45
files:
  - src/auth/middleware.py
  - src/auth/sessions.py
abstractions:
  - "verify_token — JWT verification"
  - "require_auth — auth-enforcing decorator"
  - "create_session — start a session"
  - "get_session — look up a session"
  - "destroy_session — end a session"
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

- `verify_token` <- tests/test_auth.py::test_verify_valid_token, tests/test_auth.py::test_verify_empty_token_rejected
- `require_auth` <- tests/test_auth.py::test_require_auth_blocks_unauthenticated
- `create_session` <- tests/test_sessions.py::test_create_and_get_session
- `get_session` <- tests/test_sessions.py::test_create_and_get_session
- `destroy_session` <- tests/test_sessions.py::test_destroy_session_removes_it
- upstream: docs/auth-guide.md

## Update Triggers

Re-verify when token expiry handling, the session store, or the `require_auth`
contract changes — rerun the linked tests in tests/test_auth.py and
tests/test_sessions.py.
