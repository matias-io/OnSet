#!/usr/bin/env bash
set -euo pipefail
echo "AUTOCUE_WORKER_PID=$$"
root="$(cd -- "$(dirname -- "$0")/.." && pwd)"
runtime="$HOME/.local/share/autocue"
uv="${1:?Pass verified uv binary}"
environment="$runtime/experimental/allin1"
"$uv" venv --allow-existing --python "$runtime/venv/bin/python" "$environment"
"$uv" pip install --python "$environment/bin/python" numpy==1.23.5 cython==0.29.37 setuptools==80.9.0 wheel==0.45.1 ziglang==0.13.0 hatchling==1.27.0
cp "$root/worker/zig_compiler.py" "$environment/bin/autocue-zig-compiler.py"
export CC="\"$environment/bin/python\" \"$environment/bin/autocue-zig-compiler.py\" cc"
export CXX="\"$environment/bin/python\" \"$environment/bin/autocue-zig-compiler.py\" c++"
if [[ ! -f "$root/worker/requirements-allin1.lock" ]]; then
  "$uv" pip compile --python "$environment/bin/python" --no-build-isolation --generate-hashes --extra-index-url https://download.pytorch.org/whl/cpu --index-strategy unsafe-best-match "$root/worker/requirements-allin1.in" -o "$root/worker/requirements-allin1.lock" >/dev/null
fi
"$uv" pip sync --python "$environment/bin/python" --no-build-isolation --extra-index-url https://download.pytorch.org/whl/cpu --index-strategy unsafe-best-match "$root/worker/requirements-allin1.lock"
"$uv" pip check --python "$environment/bin/python"
"$environment/bin/python" -c 'import imageio_ffmpeg,os,sys;target=os.path.join(os.path.dirname(sys.executable),"ffmpeg");os.path.exists(target) or os.symlink(imageio_ffmpeg.get_ffmpeg_exe(),target)'
