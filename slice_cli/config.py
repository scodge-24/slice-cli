from __future__ import annotations

from dataclasses import dataclass

from .context import Ctx
from .persistence import _parse_yaml


_AMBIGUITY_VALUES = ("strict", "best_effort")


@dataclass(frozen=True)
class SliceConfig:
    """Contents of slices/config.yaml. Missing file defaults to strict."""
    ambiguity: str = "strict"


def load_config(ctx: Ctx) -> SliceConfig:
    """Load slices/config.yaml. Absent file -> strict defaults.

    Raises ValueError on an invalid context.ambiguity value, naming the bad
    value, the allowed values, and the config path.
    """
    path = ctx.slices_dir / "config.yaml"
    if not path.exists():
        return SliceConfig()
    raw = _parse_yaml(path.read_text(encoding="utf-8"), ctx.rel(path))
    if not isinstance(raw, dict):
        return SliceConfig()
    context = raw.get("context")
    if not isinstance(context, dict):
        return SliceConfig()
    ambiguity = str(context.get("ambiguity", "strict")).strip() or "strict"
    if ambiguity not in _AMBIGUITY_VALUES:
        raise ValueError(
            f"invalid context.ambiguity '{ambiguity}' in {ctx.rel(path)}; "
            f"allowed: {', '.join(_AMBIGUITY_VALUES)}"
        )
    return SliceConfig(ambiguity=ambiguity)
