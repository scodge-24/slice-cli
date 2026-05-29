from __future__ import annotations

import os
import subprocess
from pathlib import Path


class Ctx:
    """Lazily-resolved repo paths. Create once, pass everywhere."""

    def __init__(self, repo: str | None = None, slices_dir: str | None = None):
        self._repo = Path(repo).resolve() if repo else None
        self._slices = Path(slices_dir).resolve() if slices_dir else None

    @property
    def repo_root(self) -> Path:
        if self._repo is None:
            env = os.environ.get("SLICES_REPO_ROOT")
            if env:
                self._repo = Path(env).resolve()
            else:
                current = Path.cwd()
                for parent in [current, *current.parents]:
                    if (parent / ".git").exists():
                        self._repo = parent
                        break
                else:
                    raise RuntimeError(
                        "Not inside a git repository. Set SLICES_REPO_ROOT or use --repo."
                    )
        return self._repo

    @property
    def slices_dir(self) -> Path:
        if self._slices is None:
            env = os.environ.get("SLICES_DIR")
            self._slices = Path(env).resolve() if env else self.repo_root / "slices"
        return self._slices

    @property
    def index_path(self) -> Path:
        return self.slices_dir / "INDEX.md"

    @property
    def docs_manifest_path(self) -> Path:
        return self.slices_dir / "DOCS.yaml"

    def git(self, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["git", *args],
            cwd=self.repo_root,
            capture_output=True,
            text=True,
            check=check,
        )

    def head_sha(self) -> str:
        try:
            return self.git("rev-parse", "HEAD").stdout.strip()
        except (OSError, subprocess.CalledProcessError):
            return "unknown"


    def rel(self, path: Path) -> str:
        try:
            return str(path.relative_to(self.repo_root))
        except ValueError:
            return str(path)
