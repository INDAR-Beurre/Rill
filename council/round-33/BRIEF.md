# Round 33 — what you are scoring

You are one of twenty critics. You score ONE facet of this browser out of 10.
You do not fix anything. You do not run `cargo build`, `cargo test`, or the
browser: the supervisor already did, and concurrent runs deadlock `target/`
and kill each other's processes. Read files and look at pictures.

## What this is

`/home/alex/.t3/scratch/2026-11-17-build-this-browser-https-github-b500011a/search-linux`
is a from-scratch Linux re-creation of a macOS browser called Search
(github.com/driceroland/Search). Rust + GTK3 + WebKitGTK. No Chromium. The
target the user set: one idle tab should cost about 100 MB of the browser's
own memory, and the whole thing should reach 9/10 on every facet.

## Evidence you must actually look at

- `reference/reference-window.png` — the real macOS app, 1500x917. This is the
  thing being imitated. Also `reference/reference-1740.png` (same window,
  smaller) and anything else in `reference/`.
- `council/round-33/*.png` — 15 states of this build, light, 1500x917.
- `council/round-33-dark/*.png` — the same 15 states, dark.
- `council/round-33/manifest.json` — what each state is and how it was reached.
- `build/stress-report.json` — the full passing run: 73 checks, every number
  the docs quote. Read it with python3/json, not by eye.
- `src/*.rs`, `assets/*.js`, `tools/*.py`, `docs/*.md`, `README.md`.

To read a picture, use the `read` tool on the absolute path. To measure a
pixel or crop a region, ImageMagick is installed and PIL is NOT:
`magick <file> -crop 120x40+8+40 +repage /tmp/crop.png` then read the crop;
`magick <file> -crop 1x1+700+10 -format "%[pixel:p{0,0}]" info:` for a colour.
Crops are how you tell a 2% ghost from a real fill — do not guess from the
full-size picture.

## How to score

- 10 = indistinguishable from a shipped, professional browser in this facet.
- 9 = the bar. A picky reviewer finds nothing they would file a bug about.
- 7 = good, with specific things a reviewer would file.
- 5 = works but visibly unfinished.
- Below 5 = broken or absent.

Be harsh. A score with no cited evidence is worthless: every point you take
off must name a file and line, or a picture and a pixel region, or a check
name in the report. Quote the exact string or number. If you claim something
is missing, grep for it first and say what you searched.

Do not pad. Do not praise. Do not suggest a rewrite. Name the smallest change
that would move your facet up, in priority order.

## What to write

Write your report to `council/round-33/scores/<your-facet-number>-<slug>.md`
(create the directory if needed) with exactly this shape:

```
# <facet name>
score: <n.n>/10

## What I looked at
<files with line numbers, pictures with regions, report check names>

## What is right
<at most 4 bullets, each with evidence>

## What costs points
<each: the defect, the evidence, the exact smallest fix, and how much it costs>

## The three changes that would raise this score most
1. ...
2. ...
3. ...
```

Then reply with your score, the three changes, and nothing else.
