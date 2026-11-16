#!/usr/bin/env bash
set -euo pipefail
echo "ONSET_WORKER_PID=$$"
root="$(cd -- "$(dirname -- "$0")/.." && pwd)"
runtime="$HOME/.local/share/onset"
mkdir -p "$runtime"
uv="${1:?Pass the verified uv binary path}"
chmod +x "$uv"
export UV_PYTHON_INSTALL_DIR="$runtime/python"
"$uv" python install 3.11
if [[ ! -x "$runtime/venv/bin/python" ]]; then "$uv" venv --python 3.11 "$runtime/venv"; fi
"$uv" pip sync --python "$runtime/venv/bin/python" --require-hashes --index-strategy unsafe-best-match --extra-index-url https://download.pytorch.org/whl/cpu "$root/worker/requirements-cpu.lock"
"$uv" pip check --python "$runtime/venv/bin/python"
"$runtime/venv/bin/python" -c 'import imageio_ffmpeg,os; target=os.path.join(os.path.dirname(__import__("sys").executable),"ffmpeg"); os.path.exists(target) or os.symlink(imageio_ffmpeg.get_ffmpeg_exe(),target)'
echo 'Worker installed. Model assets download on the first requested deep analysis.'
