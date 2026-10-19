#!/usr/bin/env bash
set -euo pipefail
echo "AUTOCUE_WORKER_PID=$$"
root="$(cd -- "$(dirname -- "$0")/.." && pwd)"
runtime="$HOME/.local/share/autocue"
uv="${1:?Pass the verified uv binary}"
environment="$runtime/experimental/edmformer"
"$uv" venv --allow-existing --python "$runtime/venv/bin/python" "$environment"
"$uv" pip sync --python "$environment/bin/python" --extra-index-url https://download.pytorch.org/whl/cpu --index-strategy unsafe-best-match "$root/worker/requirements-edmformer.lock"
"$uv" pip check --python "$environment/bin/python"
"$environment/bin/python" "$root/worker/provision_edmformer.py" "$runtime/experimental/edmformer-assets"
"$environment/bin/python" -c 'import imageio_ffmpeg,os,sys;target=os.path.join(os.path.dirname(sys.executable),"ffmpeg");os.path.exists(target) or os.symlink(imageio_ffmpeg.get_ffmpeg_exe(),target)'
