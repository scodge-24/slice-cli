"""Tests for the auth-service slice (sessions)."""

from src.auth.sessions import create_session, get_session, destroy_session


def test_create_and_get_session():
    sid = create_session("user-1")
    sess = get_session(sid)
    assert sess is not None
    assert sess["user_id"] == "user-1"


def test_destroy_session_removes_it():
    sid = create_session("user-2")
    destroy_session(sid)
    assert get_session(sid) is None
