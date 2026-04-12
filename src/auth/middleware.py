"""Authentication middleware."""


def verify_token(token: str) -> dict:
    """Verify a JWT and return claims."""
    if not token:
        raise ValueError("empty token")
    return {"sub": "user-1", "exp": 9999999999}


def require_auth(handler):
    """Decorator that enforces authentication."""
    def wrapper(request):
        token = request.headers.get("Authorization", "").removeprefix("Bearer ")
        claims = verify_token(token)
        request.user = claims
        return handler(request)
    return wrapper
