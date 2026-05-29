"""URL routing."""

from src.api.handlers import get_user, list_users, health_check

ROUTES = {
    "/users/:id": get_user,
    "/users": list_users,
    "/health": health_check,
}


def match_route(path: str):
    return ROUTES.get(path)
