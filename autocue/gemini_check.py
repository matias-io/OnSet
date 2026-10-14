#!/usr/bin/env python
"""
gemini_check.py - ask Gemini (via the `agy` CLI) to locate drop / breakdown / outro
by actually listening to each track, and compare with autocue's energy analysis.

  gemini_check.py analysis.json [--out gemini.json] [--jobs 3] [--model gemini-3.8-flash-high]
"""
import argparse, base64, json, os, re, subprocess, sys, time, urllib.request, urllib.error
import threading
from concurrent.futures import ThreadPoolExecutor, as_completed

AGY_LOCK = threading.Lock()          # agy sessions are serialized: concurrency makes the agent wander
API_DOWN_UNTIL = 0.0                 # circuit breaker: skip the API for a while after an overload
AGY_QUOTA_UNTIL = 0.0                # subscription quota exhausted: sleep until it resets
API_BREAK = 6 * 3600
from pathlib import Path

PROMPT = ('Listen to the audio file {name}. It is a 4/4 dance track. '
          'Give me timestamps (m:ss) for: (1) the first drop (the first moment the full kick drum and bass play together '
          'continuously after the intro or build - the first one, even if a later drop is bigger or the main hook/melody '
          'arrives later; a verse sung over the full beat still counts), (2) the first breakdown after that drop (where the kick/bass drops out '
          'for 8+ bars), (3) the second drop after that breakdown if any, (4) the start of the outro if there is one (where the '
          'main elements leave and the track strips down for mixing out). Also tell me the BPM, whether there is singing before '
          'the first drop, whether there is singing in the last 30 seconds, a club energy level 1-10, and a short sub-genre label. '
          'Reply as JSON only: {{"drop":"m:ss","breakdown":"m:ss or null","second_drop":"m:ss or null","outro":"m:ss or null",'
          '"bpm":128,"vocals_intro":false,"vocals_outro":false,"energy":7,"genre":"...","confidence":"low|medium|high","notes":"..."}}')


KEY_FILE = Path.home() / ".config" / "autocue" / "gemini_api_key"


def agy_exe():
    """`agy` on PATH, else the Windows installer's default location."""
    import shutil
    exe = shutil.which("agy")
    if exe:
        return exe
    win = Path(os.environ.get("LOCALAPPDATA", "")) / "agy" / "bin" / "agy.exe"
    return str(win) if win.exists() else "agy"


def api_key():
    k = os.environ.get("GEMINI_API_KEY")
    if not k and KEY_FILE.exists():
        k = KEY_FILE.read_text().strip()
    return k


def ask_api(path, model, proxy_dir=None, timeout=300, key=None, attempts=8):
    """Direct Gemini API call: audio inline, JSON output forced, temperature 0. No agent, no tools."""
    p = Path(path)
    if proxy_dir:
        p = Path(proxy_dir) / (p.stem + ".mp3")
    mime = {".mp3": "audio/mp3", ".flac": "audio/flac", ".wav": "audio/wav", ".ogg": "audio/ogg",
            ".m4a": "audio/aac", ".aiff": "audio/aiff"}.get(p.suffix.lower(), "audio/mp3")
    body = {
        "contents": [{"parts": [
            {"inline_data": {"mime_type": mime, "data": base64.b64encode(p.read_bytes()).decode()}},
            {"text": PROMPT.format(name=p.name)},
        ]}],
        "generationConfig": {"temperature": 0, "responseMimeType": "application/json"},
    }
    url = f"https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
    req = urllib.request.Request(url, data=json.dumps(body).encode(), method="POST",
                                 headers={"Content-Type": "application/json", "X-goog-api-key": key})
    t0 = time.time()
    for attempt in range(attempts):
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                out = json.loads(resp.read())
            break
        except urllib.error.HTTPError as e:
            if e.code in (429, 500, 503) and attempt < attempts - 1:
                time.sleep(min(120, 15 * (attempt + 1)))
                continue
            return {"error": f"http {e.code}", "raw": e.read().decode()[:500]}
        except Exception as e:  # timeout, connection
            if attempt < attempts - 1:
                time.sleep(10)
                continue
            return {"error": str(e)[:200]}
    try:
        text = out["candidates"][0]["content"]["parts"][0]["text"]
        d = json.loads(text)
    except Exception:
        return {"error": "bad response", "raw": json.dumps(out)[:500]}
    d["seconds"] = round(time.time() - t0, 1)
    d["model"] = model
    return d


def ask_hybrid(path, api_model, agent_model, proxy_dir, key):
    """API first (fast, same model); on overload fall back to a single sandboxed agy session."""
    global API_DOWN_UNTIL
    if time.time() >= API_DOWN_UNTIL:
        d = ask_api(path, api_model, proxy_dir, key=key, attempts=2)
        if "error" not in d or not str(d["error"]).startswith("http"):
            d["via"] = "api"
            return d
        API_DOWN_UNTIL = time.time() + API_BREAK
        print(f"  API overloaded ({d['error']}) - using agy for the next {API_BREAK // 60} min", flush=True)
    else:
        d = {"error": "api skipped (circuit open)"}
    with AGY_LOCK:
        if time.time() < AGY_QUOTA_UNTIL:
            wait = AGY_QUOTA_UNTIL - time.time()
            print(f"  agy quota exhausted - sleeping {wait / 60:.0f} min until it resets", flush=True)
            time.sleep(wait)
        d2 = ask(path, agent_model, proxy_dir)
    d2["via"] = "agy"
    if "error" in d2:
        d2["api_error"] = d["error"]
    return d2


def safe_stem(stem, limit=80):
    """ASCII-only, bracket-free file stem: the agent's file tool and PowerShell both trip on
    [brackets], unicode dashes and full-width characters in the original YouTube titles."""
    s = re.sub(r"[^A-Za-z0-9._-]+", "_", stem).strip("_")
    return s[:limit] or "track"


def proxy_for(path, proxy_dir):
    """Return the 64k mono MP3 proxy for `path` inside proxy_dir (safe name), creating it with
    ffmpeg if it does not exist yet."""
    p = Path(path)
    proxy_dir = Path(proxy_dir)
    proxy_dir.mkdir(parents=True, exist_ok=True)
    q = proxy_dir / (safe_stem(p.stem) + ".mp3")
    if not q.exists():
        legacy = Path(__file__).parent / "proxies" / (p.stem + ".mp3")   # proxies made earlier under the original names
        src = legacy if legacy.exists() else p
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", str(src), "-ac", "1", "-ar", "22050", "-b:a", "64k", str(q)],
                       check=True, capture_output=True)
    return q


def ask(path, model, proxy_dir=None, timeout=900):
    p = Path(path)
    if proxy_dir:   # small mono mp3, safe name: faster upload, same timestamps
        p = proxy_for(p, proxy_dir)
    # absolute path in the prompt: with just a file name the agent first runs a shell command to
    # locate the file, which headless print mode denies - and the whole turn aborts (agy 1.2.7)
    cmd = [agy_exe(), "--sandbox", "--model", model, "--add-dir", str(p.parent), f"-p={PROMPT.format(name=str(p))}"]
    t0 = time.time()
    try:
        proc = subprocess.run(cmd, cwd=p.parent, capture_output=True, text=True, timeout=timeout)
        out, err = proc.stdout, proc.stderr
    except subprocess.TimeoutExpired:
        return {"error": "timeout"}
    except Exception as e:  # network / OS hiccup: record and move on, the supervisor retries
        return {"error": f"exec: {e}"[:200]}
    m = re.search(r"\{.*\}", out, re.S)
    if not m:
        q = re.search(r"quota reached.*?Resets in (?:(\d+)h)?(?:(\d+)m)?(?:(\d+)s)?", err, re.S)
        if q:
            h, mi, se = (int(x or 0) for x in q.groups())
            reset = h * 3600 + mi * 60 + se
            global AGY_QUOTA_UNTIL
            AGY_QUOTA_UNTIL = time.time() + reset + 180
            return {"error": "agy quota", "reset_s": reset, "stderr": err[-300:]}
        return {"error": "no json", "raw": out[-500:], "stderr": err[-500:], "exit": proc.returncode}
    try:
        d = json.loads(m.group(0))
    except json.JSONDecodeError:
        return {"error": "bad json", "raw": m.group(0)[:500]}
    if "drop" not in d:   # agy prints a metadata block (conversationId, ...) when the agent gave up
        return {"error": "no answer", "raw": out[-500:], "stderr": err[-300:]}
    d["seconds"] = round(time.time() - t0, 1)
    return d


def online(timeout=5):
    try:
        urllib.request.urlopen("https://www.google.com/generate_204", timeout=timeout)
        return True
    except Exception:
        return False


def wait_online():
    if online():
        return
    print("network down - waiting...", flush=True)
    while not online():
        time.sleep(30)
    print("network back", flush=True)


def to_s(v):
    if v in (None, "", "null"):
        return None
    m = re.match(r"^\s*(\d+):(\d\d(?:\.\d+)?)\s*$", str(v))
    if not m:
        return None
    return int(m.group(1)) * 60 + float(m.group(2))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("analysis")
    ap.add_argument("--out", default="gemini.json")
    ap.add_argument("--jobs", type=int, default=3)
    ap.add_argument("--model", default=None, help="API: e.g. gemini-3.8-flash (default); agy: gemini-3.8-flash-high")
    ap.add_argument("--agent", action="store_true", help="force the agy CLI even if an API key is available")
    ap.add_argument("--hybrid", action="store_true", help="API first, fall back to a single agy session on overload")
    ap.add_argument("--retry-errors", action="store_true")
    ap.add_argument("--proxy-dir", help="folder of <stem>.mp3 proxies to send instead of the originals")
    ap.add_argument("--dir", help="also ask about every audio file in this folder (not only those in analysis.json)")
    ap.add_argument("--until-done", action="store_true",
                    help="keep re-running failed tracks (waiting for the network if needed) until none are left")
    ap.add_argument("--max-passes", type=int, default=12)
    args = ap.parse_args()

    results = json.loads(Path(args.analysis).read_text()) if Path(args.analysis).exists() else []
    cache = json.loads(Path(args.out).read_text()) if Path(args.out).exists() else {}
    paths = [r["path"] for r in results]
    if args.dir:
        for f in sorted(Path(args.dir).iterdir()):
            if f.suffix.lower() in (".flac", ".mp3", ".m4a", ".wav", ".aiff", ".ogg") and str(f) not in paths:
                paths.append(str(f))
    todo = [p for p in paths if p not in cache or (args.retry_errors and "error" in cache[p])]
    print(f"{len(todo)} tracks to ask, {len(cache)} cached")
    for pass_no in range(1, args.max_passes + 1):
        if pass_no > 1:
            todo = [p for p in paths if p not in cache or "error" in cache[p]]
            if not todo:
                break
            print(f"\npass {pass_no}: retrying {len(todo)} failed track(s)", flush=True)
            time.sleep(60)
        wait_online()
        run_pass(todo, cache, args)
        if not args.until_done:
            break
    compare(paths, cache, results)


def run_pass(todo, cache, args):

    key = None if args.agent else api_key()
    if key and args.hybrid:
        model = args.model or "gemini-3.8-flash"
        print(f"hybrid: Gemini API ({model}) first, agy ({model}-high, serialized) on overload")
        fn = lambda p: ask_hybrid(p, model, model + "-high", args.proxy_dir, key)
    elif key:
        model = args.model or "gemini-3.8-flash"
        print(f"using Gemini API directly ({model})")
        fn = lambda p: ask_api(p, model, args.proxy_dir, key=key)
    else:
        model = args.model or "gemini-3.8-flash-high"
        print(f"using agy CLI ({model})")
        fn = lambda p: ask(p, model, args.proxy_dir)
    consecutive_errors = 0
    with ThreadPoolExecutor(args.jobs) as ex:
        futs = {ex.submit(fn, p): p for p in todo}
        for i, f in enumerate(as_completed(futs), 1):
            p = futs[f]
            d = f.result()
            if "error" in d and not online():   # don't burn retries while the network is down
                wait_online()
            consecutive_errors = consecutive_errors + 1 if ("error" in d and d["error"] != "agy quota") else 0
            if consecutive_errors >= 8:
                print("8 failures in a row - backing off 15 min", flush=True)
                time.sleep(900)
                consecutive_errors = 0
            cache[p] = d
            Path(args.out).write_text(json.dumps(cache, indent=1))
            tag = d.get("error") or f"drop {d.get('drop')} break {d.get('breakdown')} outro {d.get('outro')} [{d.get('confidence')}] {d.get('seconds')}s via {d.get('via', d.get('model', 'agy'))}"
            print(f"[{i}/{len(todo)}] {Path(p).name[:55]:55} {tag}", flush=True)


def compare(paths, cache, results):
    print()
    print(f"{'track':48} {'bpm':>5} {'gBPM':>5} | {'D ours':>6} {'D gem':>6} | {'E ours':>6} {'E gem':>6} | {'F ours':>6} {'F gem':>6} | conf")
    def fmt(t):
        return "-" if t is None else f"{int(t)//60}:{int(t)%60:02d}"
    disagreements = 0
    for r in results:
        g = cache.get(r["path"], {})
        cues = {(c[0] if len(c) >= 3 else c[0][0]): (c[2] if len(c) >= 3 else c[1]) for c in r["cues"]}
        bar = 4 * 60 / r["bpm"] if r.get("bpm") else 2
        row = []
        bad = False
        for letter, key in (("D", "drop"), ("E", "breakdown"), ("F", "outro")):
            ours, theirs = cues.get(letter), to_s(g.get(key))
            mark = ""
            if ours is not None and theirs is not None and abs(ours - theirs) > 2 * bar:
                mark = "!"; bad = True
            elif (ours is None) != (theirs is None):
                mark = "?"
            row.append(f"{fmt(ours):>6} {fmt(theirs) + mark:>6}")
        disagreements += bad
        print(f"{Path(r['path']).stem[:48]:48} {r.get('bpm', 0):5.0f} {str(g.get('bpm', '-')):>5} | " + " | ".join(row) + f" | {g.get('confidence', g.get('error', ''))}")
    print(f"\n{disagreements} tracks with a >2-bar disagreement on a cue both methods placed ('!'); '?' = only one method placed it")


if __name__ == "__main__":
    main()
