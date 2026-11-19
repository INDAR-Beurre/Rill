#!/usr/bin/env python3
"""Self-test for the retry policy in tools/bench.py. No browser needed.

    python3 tools/test_bench.py
"""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import bench  # noqa: E402


class Retryable(unittest.TestCase):
    def test_read_only_is_retryable(self):
        for line in ("probe", "text", "tabs", "pins x"):
            self.assertTrue(bench.retryable(line, "Connection reset by peer"))
            self.assertTrue(bench.retryable(line, "Broken pipe"))

    def test_mutating_is_not_retryable(self):
        for line in ("open https://a", "go https://b", "select 3", "close 3"):
            self.assertFalse(bench.retryable(line, "Connection reset by peer"))
            self.assertFalse(bench.retryable(line, "the browser closed the socket"))

    def test_other_errors_are_not_retryable(self):
        self.assertFalse(bench.retryable("probe", "bad answer to 'probe'"))
        self.assertFalse(bench.retryable("probe", "no answer in time"))


class Must(unittest.TestCase):
    def setUp(self):
        self.b = object.__new__(bench.Bench)
        self.calls = []
        self.b.reconnect = lambda: self.calls.append("reconnect")

    def failing(self, line):
        if not self.calls:
            self.calls.append(line)
            raise bench.BenchError(f"the browser closed the socket on: {line} (Broken pipe)")
        self.calls.append(line)
        return {"ok": True}

    def test_reset_replays_read_only_once(self):
        self.b.command = self.failing
        self.assertTrue(self.b.must("probe")["ok"])
        self.assertEqual(self.calls[1], "reconnect")

    def test_reset_does_not_replay_mutating(self):
        self.b.command = self.failing
        with self.assertRaises(bench.BenchError):
            self.b.must("open https://example.com")
        self.assertNotIn("reconnect", self.calls)

    def test_timeout_is_retryable_even_mutating(self):
        def slow(line):
            if not self.calls:
                self.calls.append(line)
                raise bench.BenchError("no answer in time")
            self.calls.append(line)
            return {"ok": True}

        self.b.command = slow
        self.assertTrue(self.b.must("go https://example.com")["ok"])


if __name__ == "__main__":
    unittest.main()
