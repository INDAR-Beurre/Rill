#!/usr/bin/env python3
"""Drive a running Search over its bench socket.

The browser writes a Unix socket when it is started with SEARCH_BENCH=1 (or
--bench). One command per line, one JSON answer per line, exactly as the Mac
build's ./bench speaks to it.

    python3 tools/bench probe
    python3 tools/bench open https://example.com
    python3 tools/bench text
    python3 tools/bench shot /tmp/page.png
"""
import json
import os
import socket
import sys
import time

DEFAULT_SOCKET = os.path.join(
    os.environ.get("XDG_RUNTIME_DIR", "/tmp"), "search-bench.sock"
)


class BenchError(RuntimeError):
    pass


class Bench:
    def __init__(self, path=None, timeout=30.0):
        self.path = path or os.environ.get("SEARCH_BENCH_SOCKET", DEFAULT_SOCKET)
        self.timeout = timeout
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(timeout)
        try:
            self.sock.connect(self.path)
        except OSError as error:
            raise BenchError(
                f"no bench socket at {self.path}: start the browser with SEARCH_BENCH=1 ({error})"
            )
        self.reader = self.sock.makefile("rw", buffering=1)

    def command(self, line):
        if "\n" in line:
            raise ValueError("one command per line")
        self.reader.write(line + "\n")
        self.reader.flush()
        answer = self.reader.readline()
        if not answer:
            raise BenchError(f"the browser closed the socket on: {line}")
        try:
            return json.loads(answer)
        except json.JSONDecodeError as error:
            raise BenchError(f"bad answer to {line!r}: {answer!r} ({error})")

    def must(self, line):
        answer = self.command(line)
        if not answer.get("ok"):
            raise BenchError(f"{line}: {answer.get('error')}")
        return answer

    # -- convenience -----------------------------------------------------
    def probe(self):
        return self.must("probe")

    def open(self, url):
        return self.must(f"open {url}")["id"]

    def go(self, url):
        return self.must(f"go {url}")

    def select(self, tab_id):
        return self.must(f"select {tab_id}")

    def close(self, tab_id):
        return self.must(f"close {tab_id}")

    def text(self):
        answer = self.must("text")
        value = answer.get("text")
        if isinstance(value, str):
            try:
                return json.loads(value)
            except json.JSONDecodeError:
                return {"text": value}
        return value or {}

    def wait_for(self, predicate, timeout=20.0, interval=0.1):
        """Wait until predicate(probe) is true; returns the probe."""
        deadline = time.time() + timeout
        last = None
        while time.time() < deadline:
            last = self.probe()
            if predicate(last):
                return last
            time.sleep(interval)
        raise BenchError(f"timed out waiting; last probe: {json.dumps(last)[:400]}")

    def wait_loaded(self, tab_id=None, timeout=25.0):
        def ready(probe):
            if tab_id is None:
                return not probe.get("loading", False) and bool(probe.get("url"))
            for tab in probe.get("tabs", []):
                if tab.get("id") == tab_id:
                    return not tab.get("loading", False) and bool(tab.get("url"))
            return False

        return self.wait_for(ready, timeout=timeout)

    def disconnect(self):
        try:
            self.sock.close()
        except OSError:
            pass


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    bench = Bench()
    command = argv[1]
    argument = " ".join(argv[2:])
    if command == "shot":
        bench.command("shot")  # keep the connection warm
    answer = bench.command(f"{command} {argument}".strip())
    print(json.dumps(answer, indent=2 if os.isatty(1) else None))
    return 0 if answer.get("ok", True) else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
