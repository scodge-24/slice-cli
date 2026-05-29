---
doc_id: data-model
title: Data Model
tags: [models, schema]
---

# Data Model

## User

The `User` dataclass in `src/models/user.py` has four fields:

- `id` (str) — unique identifier
- `name` (str) — display name
- `email` (str) — optional, defaults to empty
- `active` (bool) — defaults to True
