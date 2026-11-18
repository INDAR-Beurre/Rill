#!/usr/bin/env python3
"""Print the figures docs/LIMITS.md quotes, straight from the last run.

The limits page said "the last run of tools/stress.py" and then went stale,
because the numbers were copied by hand. This prints the table so it can be
copied from a machine instead of from memory.
"""
import json
from pathlib import Path

REPORT = Path(__file__).resolve().parent.parent / "build" / "stress-report.json"


def by_name(report, name):
    for result in report["results"]:
        if result["name"] == name:
            return result
    return None


def usage(result):
    if not result:
        return {}
    if isinstance(result.get("memory"), dict):
        return result["memory"]
    try:
        return json.loads(result.get("detail") or "{}")
    except json.JSONDecodeError:
        return {}


def main():
    report = json.loads(REPORT.read_text())
    idle = usage(by_name(report, "idle one tab")) or {}
    many = by_name(report, "6 tabs open")
    many_memory = usage(many)
    print(f"| one idle tab | {idle.get('pss_mb')} MB resident | {idle.get('anon_mb')} MB own |")
    if many:
        print(f"| 8 tabs | {many_memory.get('pss_mb')} MB | {many_memory.get('anon_mb')} MB |")
        print(f"| per tab | +{many.get('per_tab_mb')} MB resident | |")
    spray = by_name(report, "a memory spray is survived")
    settle = by_name(report, "the engine survives a spray and keeps working")
    if spray:
        peak = usage(spray) or {}
        print(f"| a page that sprays memory | {peak.get('pss_mb')} MB resident at its peak, "
              f"{peak.get('rss_mb')} MB RSS | {peak.get('anon_mb')} MB |")
    if settle:
        print(f"| after it settles | {usage(settle).get('pss_mb')} MB | |")
    switches = by_name(report, "tab switches stay quick")
    if switches:
        print(f"| tab switch | mean {switches.get('mean_ms')} ms, worst "
              f"{switches.get('worst_ms')} ms | |")
    panels = by_name(report, "panels open and close")
    if panels:
        print(f"| a panel opening | {panels.get('detail')} | |")
    print(f"| checks | {report['passed']} of {report['passed'] + report['failed']} | |")


if __name__ == "__main__":
    main()
