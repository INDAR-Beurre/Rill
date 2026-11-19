#!/usr/bin/env python3
"""Print the figures docs/LIMITS.md quotes, straight from the last run.

The limits page said "the last run of tools/stress.py" and then went stale,
because the numbers were copied by hand. This prints the table so it can be
copied from a machine instead of from memory.

    python3 tools/report_figures.py            # the table, for the page
    python3 tools/report_figures.py --check    # fail if the page disagrees

A result the suite no longer produces is never a crash: the row it would have
filled is left out and the reason goes to stderr, so a page quoting a figure
that vanished looks different from one quoting a figure that merely moved.
"""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REPORT = ROOT / "build" / "stress-report.json"
DOC = ROOT / "docs" / "LIMITS.md"

# How far a printed figure may drift from what the page says before the page is
# wrong. Ten percent is the promise LIMITS.md makes about itself.
TOLERANCE = 0.10


def by_name(report, name):
    for result in report.get("results") or []:
        if result.get("name") == name:
            return result
    return None


def by_suffix(report, suffix):
    """The suite names some checks after the number they used ("20 tabs open"),
    so the figure is found by what the name ends with, never by the count a
    page happened to be written with."""
    for result in report.get("results") or []:
        if (result.get("name") or "").endswith(suffix):
            return result
    return None


def usage(result):
    """The memory block of a check, from the extra dict or the detail line."""
    if not result:
        return {}
    if isinstance(result.get("memory"), dict):
        return result["memory"]
    try:
        parsed = json.loads(result.get("detail") or "{}")
    except json.JSONDecodeError:
        return {}
    return parsed if isinstance(parsed, dict) else {}


def first(*values):
    for value in values:
        if value is not None:
            return value
    return None


def figures(report):
    """Every number the page quotes, keyed by the name the table and the check
    mode both use, plus a list of results that were not there to read."""
    found, missing = {}, []

    def put(key, value, what):
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            if what not in missing:
                missing.append(what)
            return
        found[key] = float(value)

    idle = usage(by_name(report, "idle one tab"))
    put("idle pss", idle.get("pss_mb"), "idle one tab")
    put("idle anon", idle.get("anon_mb"), "idle one tab")
    # The bottom of the range the docs quote for one idle tab. It is the
    # quietest reading of the passing runs, not this run's figure, so it is a
    # constant the docs and the checker share rather than a measurement.
    found["idle anon low"] = 49.0

    # The many-tabs check is named after the tab count the run used, which is
    # why the page must not hardcode "8 tabs": the run's own name is the label.
    many = by_suffix(report, "tabs open")
    many_memory = usage(many)
    count = None
    if many:
        match = re.match(r"\d+", many.get("name") or "")
        count = int(match.group(0)) if match else None
    if count is None:
        if "tabs open" not in missing:
            missing.append("tabs open")
    else:
        found["many count"] = float(count)
    put("many pss", many_memory.get("pss_mb"), "tabs open")
    put("many anon", many_memory.get("anon_mb"), "tabs open")
    put("per tab", first((many or {}).get("per_tab_mb"), many_memory.get("per_tab_mb")),
        "tabs open (per tab)")

    spray = usage(by_name(report, "a memory spray is survived"))
    put("spray pss", spray.get("pss_mb"), "a memory spray is survived")
    put("spray rss", spray.get("rss_mb"), "a memory spray is survived")
    put("spray anon", spray.get("anon_mb"), "a memory spray is survived")
    settle = usage(by_name(report, "the engine survives a spray and keeps working"))
    put("settle pss", settle.get("pss_mb"), "the engine survives a spray and keeps working")

    # The idle minute's drift, read out of the detail line the suite writes.
    # It used to be the one row on the page the checker skipped, which is how a
    # stale figure survived there.
    idle_growth = by_name(report, "nothing grows while nothing happens") or {}
    detail = idle_growth.get("detail") or ""
    drifts = re.findall(r"\(([+-]?\d+(?:\.\d+)?)MB\)", detail)
    if len(drifts) >= 2:
        put("drift pss", float(drifts[0]), "nothing grows while nothing happens (resident)")
        put("drift anon", float(drifts[1]), "nothing grows while nothing happens (own)")

    switches = by_name(report, "tab switches stay quick") or {}
    put("switch mean", switches.get("mean_ms"), "tab switches stay quick (mean)")
    put("switch worst", switches.get("worst_ms"), "tab switches stay quick (worst)")
    put("switch p95", switches.get("p95_ms"), "tab switches stay quick (p95)")

    panels = by_name(report, "panels open and close") or {}
    mean = panels.get("mean_ms")
    if mean is None:
        # The panel check records its mean in the sentence it prints, so read it
        # from there rather than showing nothing.
        match = re.search(r"mean\s+([\d.]+)\s*ms", panels.get("detail") or "")
        mean = float(match.group(1)) if match else None
    put("panel mean", mean, "panels open and close (mean)")
    put("panel worst", first(panels.get("worst_ms"), panels.get("panel_worst_ms")),
        "panels open and close (worst)")

    for path in ("/long", "/images"):
        put(f"loads {path}", (by_name(report, f"loads {path}") or {}).get("seconds"),
            f"loads {path}")

    passed, failed = report.get("passed"), report.get("failed")
    put("checks passed", passed, "the run's pass count")
    put("checks total", (passed + failed)
        if isinstance(passed, int) and isinstance(failed, int) else None,
        "the run's pass count")

    return found, missing


def rows(found):
    """The markdown table, one row per figure that is actually there."""
    lines = []

    def want(keys):
        return all(key in found for key in keys)

    if want(["idle pss", "idle anon"]):
        lines.append(f"| one idle tab | {found['idle pss']:.0f} MB resident "
                     f"| {found['idle anon']:.0f} MB own |")
    if want(["many count", "many pss", "many anon"]):
        lines.append(f"| {found['many count']:.0f} tabs | {found['many pss']:.0f} MB "
                     f"| {found['many anon']:.0f} MB |")
    if want(["per tab"]):
        lines.append(f"| per tab | +{found['per tab']:.1f} MB resident | |")
    if want(["spray pss", "spray rss", "spray anon"]):
        lines.append(f"| a page that sprays memory | {found['spray pss']:.0f} MB resident "
                     f"at its peak, {found['spray rss']:.0f} MB RSS "
                     f"| {found['spray anon']:.0f} MB |")
    if want(["settle pss"]):
        lines.append(f"| after it settles | {found['settle pss']:.0f} MB | |")
    if want(["switch mean", "switch worst"]):
        lines.append(f"| tab switch | mean {found['switch mean']:.1f} ms, worst "
                     f"{found['switch worst']:.1f} ms | |")
    if want(["switch p95"]):
        lines.append(f"| tab switch, 95th percentile | {found['switch p95']:.1f} ms | |")
    if want(["panel mean"]):
        lines.append(f"| a panel opening | mean {found['panel mean']:.0f} ms | |")
    if want(["panel worst"]):
        lines.append(f"| a panel at its worst | {found['panel worst']:.0f} ms | |")
    if want(["checks passed", "checks total"]):
        lines.append(f"| checks | {found['checks passed']:.0f} of "
                     f"{found['checks total']:.0f} | |")
    return lines


# Which report figure a row of the page's table is about, in the order the row
# quotes them. A label that is not here is skipped, not guessed at: a wrong
# mapping would "check" the page against a figure it never claimed.
RULES = [
    (re.compile(r"^one idle tab$", re.I), ["idle pss", "idle anon"]),
    (re.compile(r"^one idle tab,", re.I), None),  # a hand-measured second config
    (re.compile(r"^(\d+) tabs$", re.I), ["many pss", "many anon"]),
    (re.compile(r"^per tab$", re.I), ["per tab"]),
    (re.compile(r"^a page that allocates without limit$", re.I),
     ["spray pss", "spray rss", "spray anon"]),
    (re.compile(r"^once it settles$", re.I), ["settle pss"]),
    (re.compile(r"^tab switch", re.I), ["switch mean", "switch p95", "switch worst"]),
    (re.compile(r"^a panel opening$", re.I), ["panel mean"]),
    (re.compile(r"^a panel at its worst$", re.I), ["panel worst"]),
    (re.compile(r"^a 400-paragraph page$", re.I), ["loads /long"]),
    (re.compile(r"^a page of 40 images$", re.I), ["loads /images"]),
    (re.compile(r"^a page that sprays memory, rss$", re.I), ["spray rss"]),
    (re.compile(r"^nothing grows while nothing happens$", re.I), ["drift anon"]),
    (re.compile(r"^checks$", re.I), ["checks passed", "checks total"]),
]

SKIP = re.compile(r"^(what|case|the difference|every figure on this page)$", re.I)
SEPARATOR = re.compile(r"^:?-{2,}:?$")


def doc_rows(path):
    """The table rows of the page: (label, figure cell), in the order printed."""
    try:
        text = path.read_text()
    except OSError as error:
        raise SystemExit(f"cannot read {path}: {error}")
    for line in text.splitlines():
        line = line.strip()
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip("|").split("|")]
        if len(cells) < 2 or all(SEPARATOR.match(cell) for cell in cells if cell):
            continue
        label = cells[0].replace("*", "").replace("`", "").strip()
        yield label, cells[1]


def decimals(number):
    return len(number.split(".")[1]) if "." in number else 0


def check(found):
    """Compare every figure the page quotes against the run it cites."""
    problems, compared = [], 0
    # DIFFERENCES.md quotes the same two memory figures in prose, and prose is
    # not a table row, so it is checked by pattern instead of being left to rot.
    try:
        prose = (ROOT / "docs" / "DIFFERENCES.md").read_text()
    except OSError:
        prose = ""
    for pattern, keys in (
        (r"resident total is about (\d+(?:\.\d+)?) MB", "idle pss"),
        (r"own anonymous memory is (\d+(?:\.\d+)?) to", "idle anon low"),
    ):
        match = re.search(pattern, prose)
        if not match or keys not in found:
            continue
        said = float(match.group(1))
        real = found[keys]
        compared += 1
        if abs(said - real) > max(0.5, abs(real) * TOLERANCE):
            problems.append(f"DIFFERENCES.md: says {said:g}, the run says {real:g} ({keys})")
    for label, cell in doc_rows(DOC):
        if SKIP.match(label):
            continue
        keys = None
        for pattern, mapped in RULES:
            match = pattern.match(label)
            if match:
                keys = mapped
                # A row labelled with a tab count also claims the run used that
                # many tabs, which the "8 tabs" row used to get wrong.
                if keys and "many count" in found and match.groups():
                    named = float(match.group(1))
                    if abs(named - found["many count"]) > 0.5:
                        problems.append(
                            f"{label}: the page says {named:.0f} tabs, "
                            f"the run used {found['many count']:.0f}")
                break
        if not keys:
            continue
        numbers = re.findall(r"\d+(?:\.\d+)?", cell)
        # A cell that names its figures is read by name, not by position.
        # "mean 1.0 ms, p95 2.5 ms, worst 26 ms" would otherwise be read as the
        # three numbers 1.0, 95 and 2.5, because the 95 in "p95" is a number too.
        named = {
            word.lower(): value
            for word, value in re.findall(
                r"\b(mean|worst|p95|resident|own|rss)\s+(\d+(?:\.\d+)?)", cell, re.I
            )
        }
        if named:
            pairs = []
            for key in keys:
                word = key.split()[-1]
                if word in named:
                    pairs.append((key, named[word]))
            if pairs:
                keys, numbers = [k for k, _ in pairs], [v for _, v in pairs]
        for key, number in zip(keys, numbers):
            if key not in found:
                # The page quotes a figure the run no longer produces: that is
                # the failure this mode exists for, not a reason to stay quiet.
                problems.append(f"{label} ({key}): the page says {number}, "
                                f"the report has no such figure")
                continue
            actual = found[key]
            # Compare against the figure as the table would print it: a page
            # cannot be more precise than the number it copies.
            shown = round(actual, decimals(number))
            compared += 1
            if shown == 0:
                if float(number) != 0:
                    problems.append(f"{label} ({key}): the page says {number}, "
                                    f"the run measured {shown:g}")
            elif abs(float(number) - shown) > TOLERANCE * abs(shown):
                problems.append(f"{label} ({key}): the page says {number}, "
                                f"the run measured {shown:g} "
                                f"({abs(float(number) - shown) / abs(shown):.0%} out)")
    if not compared:
        raise SystemExit("no figure on the page mapped to a result in the report")
    return problems


def main(argv):
    try:
        report = json.loads(REPORT.read_text())
    except FileNotFoundError:
        raise SystemExit(f"no report at {REPORT}: run python3 tools/stress.py first")
    except json.JSONDecodeError as error:
        raise SystemExit(f"{REPORT} is not JSON: {error}")

    found, missing = figures(report)
    if "--check" in argv:
        problems = check(found)
        if problems:
            print(f"{len(problems)} figure(s) in {DOC.name} disagree with the report:",
                  file=sys.stderr)
            for problem in problems:
                print(f"  {problem}", file=sys.stderr)
            return 1
        print(f"{DOC.name} agrees with the report")
        return 0

    for line in rows(found):
        print(line)
    for what in missing:
        print(f"# row omitted: no result for {what}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
