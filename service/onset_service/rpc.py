"""One JSON-RPC message per UTF-8 line; stdout contains only protocol messages."""
import contextlib
import json
import sys
import threading
from .service import Service


def serve(home=None):
    output = sys.stdout
    lock = threading.Lock()
    def emit(message):
        with lock:
            output.write(json.dumps({"jsonrpc": "2.0", **message}, ensure_ascii=False, allow_nan=False) + "\n")
            output.flush()
    service = Service(home, emit)
    try:
        for line in sys.stdin:
            request = None
            try:
                request = json.loads(line)
                if request.get("jsonrpc") != "2.0":
                    raise ValueError("Expected JSON-RPC 2.0")
                with contextlib.redirect_stdout(sys.stderr):
                    value = service.dispatch(request["method"], request.get("params"))
                emit({"id": request.get("id"), "result": value})
            except Exception as exc:
                emit({"id": request.get("id") if request else None, "error": {"code": -32000, "message": str(exc)}})
    finally:
        service.close()
