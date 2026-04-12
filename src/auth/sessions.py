"""Session management."""

_store: dict[str, dict] = {}


def create_session(user_id: str) -> str:
    sid = f"sess-{user_id}"
    _store[sid] = {"user_id": user_id, "active": True}
    return sid


def get_session(sid: str) -> dict | None:
    return _store.get(sid)


def destroy_session(sid: str) -> None:
    _store.pop(sid, None)
