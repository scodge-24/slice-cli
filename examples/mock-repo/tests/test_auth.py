"""Tests for the auth-service slice (middleware)."""

import pytest

from src.auth.middleware import verify_token, require_auth


def test_verify_valid_token():
    claims = verify_token("a-real-token")
    assert claims["sub"] == "user-1"


def test_verify_empty_token_rejected():
    with pytest.raises(ValueError):
        verify_token("")


def test_require_auth_blocks_unauthenticated():
    @require_auth
    def handler(request):
        return "ok"

    class Req:
        headers = {}

    with pytest.raises(ValueError):
        handler(Req())
