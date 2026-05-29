"""Public package surface for slice-cli."""

from .check import run_check
from .cli import build_parser, main
from .context import Ctx
from .drift import check_doc_drift
from .models import CheckResult, DocDrift, DocManifest, SliceDoc, TrackedDoc
from .persistence import load_doc_manifest, load_slice_docs
from .render import extract_sections, parse_verification

__all__ = [
    "CheckResult",
    "Ctx",
    "DocDrift",
    "DocManifest",
    "SliceDoc",
    "TrackedDoc",
    "build_parser",
    "check_doc_drift",
    "extract_sections",
    "load_doc_manifest",
    "load_slice_docs",
    "main",
    "parse_verification",
    "run_check",
]
