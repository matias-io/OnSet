"""Compatibility entry point for isolated generated-audio database assertions."""
import subprocess
import sys
from pathlib import Path

if __name__ == "__main__":
    root = Path(__file__).resolve().parent.parent
    raise SystemExit(subprocess.call([sys.executable, "-m", "pytest", "tests/test_rekordbox.py", "-q"], cwd=root))
