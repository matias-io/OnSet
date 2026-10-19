"""Cancelable owned subprocesses, including a dedicated WSL process group."""
from __future__ import annotations
import os
import queue
import subprocess
import threading
import time


def run_process(command, checkpoint, stage, timeout, *, wsl_group=False):
    flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
    proc = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True, encoding="utf-8", errors="replace", creationflags=flags)
    messages = queue.Queue()
    def drain():
        for line in proc.stdout:
            messages.put(line.rstrip())
        messages.put(None)
    threading.Thread(target=drain, daemon=True).start()
    started, group, tail, finished = time.monotonic(), None, [], False
    try:
        while True:
            checkpoint(stage, .5)
            if time.monotonic() - started > timeout:
                raise TimeoutError(f"{stage} exceeded {timeout} seconds; retry to resume installation")
            try: line = messages.get(timeout=.2)
            except queue.Empty: continue
            if line is None: break
            if wsl_group and line.startswith("AUTOCUE_WORKER_PID="):
                value = line.split("=", 1)[1]
                if value.isdigit() and int(value) > 1: group = int(value)
            else:
                tail.append(line[-300:]); del tail[:-8]
        if proc.wait():
            raise RuntimeError(stage + " failed: " + "\n".join(tail))
        finished=True
    finally:
        if not finished:
            if group:
                subprocess.run(["wsl.exe", "--exec", "kill", "-TERM", "--", f"-{group}"],
                               capture_output=True, timeout=10, creationflags=flags)
            if proc.poll() is None and os.name == "nt":
                subprocess.run(["taskkill", "/PID", str(proc.pid), "/T", "/F"],
                               capture_output=True, timeout=10, creationflags=flags)
            elif proc.poll() is None: proc.terminate()
            try: proc.wait(timeout=5)
            except subprocess.TimeoutExpired: proc.kill(); proc.wait()

    return "\n".join(tail)
