#!/usr/bin/env python3
"""Ask WebKit to compile the generated rule lists, and print what survived.

A content filter fails whole when one rule in it cannot be parsed, and WebKit's
own parser refuses constructions a general regexp engine accepts, notably any
alternation. Counting lines in network.json therefore says nothing about
whether the blocker blocks. This compiles the real thing with the same engine
the browser uses:

    python3 tools/check_shield.py
"""
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
LISTS = sorted((ROOT / "assets" / "shield").glob("*.json"))

PROBE = r'''
import gi, json, sys
gi.require_version("WebKit2", "4.1")
from gi.repository import WebKit2, GLib
paths = json.loads(sys.argv[1])
store = WebKit2.UserContentFilterStore.new(sys.argv[2])
done, failed = [], []
loop = GLib.MainLoop()
left = {"n": len(paths)}
def finished(source, result, ident):
    try:
        source.save_finish(result)
        done.append(ident)
    except GLib.Error as error:
        failed.append((ident, error.message))
    left["n"] -= 1
    if left["n"] == 0:
        loop.quit()
for ident, path in paths:
    text = open(path).read()
    store.save(ident, GLib.Bytes.new(text.encode()), None, finished, ident)
loop.run()
print(json.dumps({"ok": done, "failed": failed}))
'''


def main():
    if not LISTS:
        print("nothing to check: run tools/build_shield.py first")
        return 1
    work = pathlib.Path(tempfile.mkdtemp(prefix="shield-check-"))
    try:
        pairs = [[f"check-{index}", str(path)] for index, path in enumerate(LISTS)]
        script = work / "probe.py"
        script.write_text(PROBE)
        try:
            result = subprocess.run(
                [sys.executable, str(script), json.dumps(pairs), str(work / "store")],
                capture_output=True,
                text=True,
                timeout=300,
            )
        except subprocess.TimeoutExpired:
            print("the engine did not finish compiling in five minutes")
            return 1
        if result.returncode != 0:
            print(result.stderr.strip()[-2000:])
            print("is python3-gobject with WebKit2 4.1 installed?")
            return 1
        answer = json.loads(result.stdout.strip().splitlines()[-1])
        for name, why in answer["failed"]:
            index = int(name.split("-")[-1])
            rules = len(json.loads(LISTS[index].read_text()))
            print(f"  FAIL {LISTS[index].name}: {rules} rules lost: {why}")
        for name in answer["ok"]:
            index = int(name.split("-")[-1])
            rules = len(json.loads(LISTS[index].read_text()))
            print(f"  ok   {LISTS[index].name}: {rules} rules compile")
        total = sum(len(json.loads(path.read_text())) for index, path in enumerate(LISTS)
                    if f"check-{index}" in answer["ok"])
        print(f"{total} rules the engine accepts, {len(answer['failed'])} lists rejected")
        return 1 if answer["failed"] else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
