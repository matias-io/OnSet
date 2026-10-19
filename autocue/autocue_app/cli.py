import argparse
import json
import time
from pathlib import Path
from .service import Service


def main():
    ap = argparse.ArgumentParser(description="Autocue DJ preparation services")
    ap.add_argument("--home", help="Application data directory")
    sub = ap.add_subparsers(dest="command", required=True)
    sub.add_parser("rpc")
    sub.add_parser("doctor")
    sub.add_parser("tracks")
    sub.add_parser("workspace-health")
    sub.add_parser("workspace-backups")
    sub.add_parser("workspace-backup")
    imp = sub.add_parser("import")
    imp.add_argument("paths", nargs="+")
    analysis = sub.add_parser("analyze")
    analysis.add_argument("track_ids", nargs="*")
    analysis.add_argument("--light", action="store_true", help="Explicitly skip local deep models")
    analysis.add_argument("--wait", action="store_true")
    analysis.add_argument("--experimental", nargs="*", choices=["edmformer", "allin1"], default=[])
    for name in ("transitions", "plan", "validate-plan", "preview", "rekordbox-preview", "rekordbox-apply", "restore", "benchmark", "review-export", "import-plan", "rekordbox-import", "workspace-restore-preview", "workspace-restore", "replan", "review-queue", "import-workspace", "relocate-cache", "genre-suggestions"):
        cmd = sub.add_parser(name)
        cmd.add_argument("--params", help="JSON argument file")
    args = ap.parse_args()
    if args.command == "rpc":
        from .rpc import serve
        serve(args.home)
        return
    service = Service(args.home)
    try:
        params = json.loads(Path(args.params).read_text(encoding="utf-8")) if getattr(args, "params", None) else {}
        if args.command == "import":
            params = {"paths": args.paths}
        if args.command == "analyze":
            params = {"track_ids": args.track_ids, "deep": not args.light, "experimental": args.experimental}
        method = "export" if args.command == "review-export" else args.command
        result = service.dispatch(method, params)
        if args.command == "analyze":
            while True:
                result = next(j for j in service.store.jobs() if j["id"] == result["id"])
                if result["state"] not in {"queued", "running"}:
                    break
                time.sleep(.2)
        print(json.dumps(result, indent=2, ensure_ascii=False))
    finally:
        service.close()


if __name__ == "__main__":
    main()
