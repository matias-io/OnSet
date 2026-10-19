"""Controlled local WSL worker. It never receives account credentials."""
from __future__ import annotations
import json
import subprocess
import sys
import queue
import threading
from pathlib import Path


def wsl_path(path):
    p = Path(path).resolve().as_posix()
    if len(p) > 2 and p[1] == ":":
        return "/mnt/" + p[0].lower() + p[2:]
    return p


def windows_path(path):
    if path.startswith("/mnt/") and len(path) > 6:
        return path[5].upper() + ":" + path[6:]
    return path

def map_output_paths(value):
    if isinstance(value,dict):return {k:map_output_paths(v) for k,v in value.items()}
    if isinstance(value,list):return [map_output_paths(v) for v in value]
    if isinstance(value,str) and value.startswith('/mnt/'):return windows_path(value)
    return value


def worker_file(name):
    root = Path(__file__).resolve().parent.parent
    for directory in (root / "worker", root.parent / "worker"):
        candidate = directory / name
        if candidate.is_file():
            return candidate
    raise FileNotFoundError(f"Worker asset missing: {name}")


# Models every deep analysis requests; experimental ones are opt-in.
REQUIRED = ("beat_this", "demucs", "essentia", "genre")


def run_deep(audio, home, checkpoint, experimental=(), model_dir=None):
    script = worker_file("deep_worker.py")
    request = {"schema_version": 1, "audio": wsl_path(audio), "cache": wsl_path(model_dir or Path(home) / "models"),
               "output": wsl_path(Path(audio).parent / "deep"), "models": list(REQUIRED) + [m for m in experimental if m in {"edmformer", "allin1"}]}
    result = {"models": {}}
    try:
        from .processes import run_process
        linux_home = run_process(["wsl.exe", "--exec", "printenv", "HOME"], checkpoint,
                                 "starting WSL runtime", 60).strip()
        if not linux_home.startswith("/") or "\n" in linux_home:
            raise RuntimeError("Could not locate the WSL analysis runtime; run worker setup")
        command = ["wsl.exe", "--exec", linux_home + "/.local/share/onset/venv/bin/python", wsl_path(script)]
        proc = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                text=True, encoding="utf-8", creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        proc.stdin.write(json.dumps(request) + "\n")
        proc.stdin.close()
        # A monitor thread drains stderr; model libraries can be noisy.
        errors = []
        messages = queue.Queue()
        worker_pid = None
        worker_group = None
        def drain():
            for line in proc.stderr:
                errors.append(line[-500:])
                del errors[:-20]
        threading.Thread(target=drain, daemon=True).start()
        def read_messages():
            for line in proc.stdout:
                messages.put(line)
            messages.put(None)
        threading.Thread(target=read_messages, daemon=True).start()
        try:
            stage = "starting"
            while True:
                checkpoint("stems/models:" + stage, .5)
                try:
                    line = messages.get(timeout=.5)
                except queue.Empty:
                    continue
                if line is None:
                    break
                try:
                    event = json.loads(line)
                except ValueError:
                    continue
                if event.get("type") == "progress":
                    stage = event["model"]
                    checkpoint("stems/models:" + event["model"], .5)
                elif event.get("type") == "ready":
                    worker_pid = int(event["pid"])
                    if event.get("group") == worker_pid: worker_group = worker_pid
                elif event.get("type") == "result":
                    result = event["result"]
            code = proc.wait()
            if code and not result["models"]:
                raise RuntimeError("WSL worker failed: " + " ".join(errors)[-600:])
            if code:
                result['models']['worker_finalize']={'state':'failed','error':'Worker exited during finalization: '+ ' '.join(errors)[-600:]}
        finally:
            if proc.poll() is None:
                if worker_pid and worker_pid > 1:
                    try:
                        subprocess.run(["wsl.exe", "--exec", "kill", "-TERM", "--", str(-worker_group if worker_group else worker_pid)], capture_output=True, timeout=10,
                                       creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
                    except (OSError, subprocess.TimeoutExpired):
                        pass
                proc.terminate()
        result["stems"] = {k: windows_path(v) for k, v in result.get("stems", {}).items()}
        for name in request["models"]:
            result["models"].setdefault(name, {"state": "failed", "error": "Worker ended without reporting this model"})
    except (OSError, RuntimeError, subprocess.TimeoutExpired) as exc:
        result = {"models": {m: {"state": "unavailable", "error": str(exc)} for m in request["models"]}}
    return map_output_paths(result)
