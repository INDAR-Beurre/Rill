"""Turn a round's critic reports into one number, with the floor rule applied.

    python3 tools/score.py council/round-32

Reads `council/round-<n>/critic-*.md`, one file per facet, each in the shape the
critic brief asks for:

    FACET: <name>
    SCORE: <0-10>
    LOOKED AT: ...
    WHY: ...
    TOP FIXES: ...

Writes `scores.json` beside them and prints the mean, the floor failures and the
lowest three facets. A facet with no report is a missing facet, not a zero: the
run cannot be called finished while one is missing, so it is reported as such.
"""
import json
import pathlib
import re
import sys

# The weights the council skill names: the product is judged on fidelity and on
# whether the loop works, with the ten craft facets together worth as much as
# those two.
WEIGHTS = {
    "fidelity": 0.15,
    "core loop": 0.10,
    "performance": 0.10,
    "memory": 0.10,
    "robustness": 0.10,
    "documentation": 0.10,
    "craft": 0.35,
}

FACETS = [
    (1, "First impression at a glance", "craft"),
    (2, "Layout and geometry", "craft"),
    (3, "Typography", "craft"),
    (4, "Colour and contrast", "craft"),
    (5, "Tab row craft", "craft"),
    (6, "Pinned tabs", "craft"),
    (7, "Address field / omnibox", "craft"),
    (8, "Panels", "craft"),
    (9, "Settings", "craft"),
    (10, "Reading mode", "craft"),
    (11, "Dark mode", "craft"),
    (12, "Empty and error states", "craft"),
    (13, "Keyboard and shortcuts", "craft"),
    (14, "Performance and startup", "performance"),
    (15, "Memory", "memory"),
    (16, "Correctness of the core loop", "core loop"),
    (17, "Robustness under abuse", "robustness"),
    (18, "Code craft and architecture", "craft"),
    (19, "Documentation and repository", "documentation"),
    (20, "Fidelity to the reference", "fidelity"),
]


def read_report(path):
    text = path.read_text()
    score = re.search(r"^\s*SCORE\s*:\s*([0-9]+(?:\.[0-9]+)?)", text, re.M)
    looked = re.search(r"^\s*LOOKED AT\s*:\s*(.+?)(?=^\s*(?:WHY|TOP FIXES)\s*:)", text, re.M | re.S)
    why = re.search(r"^\s*WHY\s*:\s*(.+?)(?=^\s*TOP FIXES\s*:)", text, re.M | re.S)
    fixes = re.search(r"^\s*TOP FIXES\s*:\s*(.+)$", text, re.M | re.S)
    if score is None:
        return None
    return {
        "score": float(score.group(1)),
        "looked_at": " ".join(looked.group(1).split()) if looked else "",
        "why": " ".join(why.group(1).split()) if why else "",
        "fixes": " ".join(fixes.group(1).split()) if fixes else "",
    }


def main():
    folder = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "council/latest")
    target = float(sys.argv[2]) if len(sys.argv) > 2 else 9.0
    reports = {}
    for path in sorted(folder.glob("critic-*.md")):
        found = read_report(path)
        if found is None:
            print(f"  {path.name}: no SCORE line, ignored")
            continue
        reports[path.stem.replace("critic-", "")] = found

    rows = []
    missing = []
    for number, name, group in FACETS:
        key = f"{number:02d}"
        if key not in reports:
            missing.append(f"{number}. {name}")
            continue
        rows.append({"facet": number, "name": name, "group": group, **reports[key]})

    print(f"{'#':>3} {'facet':<34} {'group':<14} score")
    for row in rows:
        print(f"{row['facet']:>3} {row['name']:<34} {row['group']:<14} {row['score']:>5.1f}")
    if missing:
        print("\nno report for: " + ", ".join(missing))

    # Each group is its facets' mean, then the groups are weighted against each
    # other. A group with no report is left out and its weight is shared out
    # among the groups that did report, so the mean is always over 20 facets'
    # worth of weight rather than over whichever ones happened to answer.
    group_means = {}
    group_weight = {}
    for group, weight in WEIGHTS.items():
        in_group = [row for row in rows if row["group"] == group]
        if not in_group:
            continue
        group_means[group] = sum(row["score"] for row in in_group) / len(in_group)
        group_weight[group] = weight
    if not group_means:
        print("nothing scored yet")
        return
    total_weight = sum(group_weight.values())
    mean = sum(group_means[group] * group_weight[group] for group in group_means) / total_weight
    floor_failures = [row for row in rows if row["score"] < target - 1.0]
    below = [row for row in rows if row["score"] < target]
    lowest = sorted(rows, key=lambda row: row["score"])[:3]

    print(f"\nmean {mean:.2f}/10 over {len(rows)} facets, bar {target:.1f}")
    print(f"facets below the bar: {len(below)}; more than one point below: {len(floor_failures)}")
    for row in floor_failures:
        print(f"  floor: {row['facet']} {row['name']} at {row['score']}")
    for row in lowest:
        print(f"  lowest: {row['facet']} {row['name']} at {row['score']}")
    met = mean >= target and not floor_failures and not missing
    print("BAR MET" if met else "BAR NOT MET")

    out = folder / "scores.json"
    out.write_text(json.dumps({
        "round": folder.name,
        "bar": target,
        "mean": round(mean, 2),
        "met": met,
        "missing": missing,
        "facets": rows,
    }, indent=2) + "\n")
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
