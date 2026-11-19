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
import select as selector
import socket
import sys
import time

DEFAULT_SOCKET = os.path.join(
    os.environ.get("XDG_RUNTIME_DIR", "/tmp"), "search-bench.sock"
)


class BenchError(RuntimeError):
    pass


READ_ONLY = ("probe", "debug", "tabs", "ping", "text", "pins")
RESET_WORDS = ("Connection reset", "Broken pipe", "closed the socket")


def retryable(line, error):
    """Read-only commands only: after a reset or a closed socket, ask again.

    Replaying a read cannot change the page; replaying open/close/select can.
    """
    return line.split(" ", 1)[0] in READ_ONLY and any(
        word in str(error) for word in RESET_WORDS
    )


class Bench:
    def __init__(self, path=None, timeout=30.0):
        self.path = path or os.environ.get("SEARCH_BENCH_SOCKET", DEFAULT_SOCKET)
        self.timeout = timeout
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        # No socket timeout: a read that times out leaves the stream in a state
        # where every later read raises OSError("cannot read from timed out
        # object"), so one slow command would poison the whole run. The wait is
        # done with select() instead, which leaves the socket usable.
        try:
            self.sock.connect(self.path)
        except OSError as error:
            raise BenchError(
                f"no bench socket at {self.path}: start the browser with SEARCH_BENCH=1 ({error})"
            )
        self.reader = self.sock.makefile("rw", buffering=1)
        self.pending = b""

    def _readline(self, deadline):
        while b"\n" not in self.pending:
            left = deadline - time.time()
            if left <= 0:
                raise BenchError("no answer in time")
            ready, _, _ = selector.select([self.sock], [], [], left)
            if not ready:
                raise BenchError("no answer in time")
            chunk = self.sock.recv(65536)
            if not chunk:
                return b""
            self.pending += chunk
        line, _, self.pending = self.pending.partition(b"\n")
        return line + b"\n"

    def command(self, line):
        if "\n" in line:
            raise ValueError("one command per line")
        try:
            self.sock.sendall((line + "\n").encode())
        except OSError as error:
            raise BenchError(f"the browser closed the socket on: {line} ({error})")
        answer = self._readline(time.time() + self.timeout).decode(errors="replace")
        if not answer:
            raise BenchError(f"the browser closed the socket on: {line}")
        try:
            return json.loads(answer)
        except json.JSONDecodeError as error:
            raise BenchError(f"bad answer to {line!r}: {answer!r} ({error})")

    def must(self, line):
        try:
            answer = self.command(line)
        except BenchError as error:
            # A machine under memory pressure can freeze any process for
            # seconds while pages are swapped back in, and that is not the
            # browser misbehaving. Ask once more on a new connection before
            # calling it a failure. Timeouts retry anything, as before; a reset
            # or a closed socket retries only read-only commands, because a
            # mutating command may already have run.
            if "no answer in time" not in str(error) and not retryable(line, error):
                raise
            self.reconnect()
            answer = self.command(line)
        if not answer.get("ok"):
            raise BenchError(f"{line}: {answer.get('error')}")
        return answer

    # -- convenience -----------------------------------------------------
    def settle(self):
        """Let the window finish laying itself out before reading geometry."""
        return self.must("settle")

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
            if probe.get("visibleChild") == "error":
                return True
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

    def reconnect(self):
        """Drop the connection and make a new one, keeping the same settings."""
        self.disconnect()
        fresh = Bench(self.path, self.timeout)
        self.sock, self.reader, self.pending = fresh.sock, fresh.reader, fresh.pending


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
