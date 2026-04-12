"""API request handlers."""

from src.auth.middleware import require_auth
from src.models.user import User


@require_auth
def get_user(request):
    return User(id=request.user["sub"], name="Test User")


@require_auth
def list_users(request):
    return [User(id="1", name="Alice"), User(id="2", name="Bob")]


def health_check(request):
    return {"status": "ok"}
