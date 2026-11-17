# Council memo

The bar was **9/10**. It was not reached. This is what the council found, what
changed between rounds, and what the number is made of.

## Round 1 — 3.98/10

Twenty critics, one facet each. The mean of 3.98 was pulled down by a real
problem and a measurement problem, in that order of damage.

The measurement problem first, because it invalidated half the evidence: every
round-1 screenshot had the address field open. The shot script navigated, then
captured before the field was put away, so a 74% white scrim washed the whole
window and the first-impression and fidelity critics scored a screen nobody
would ever see. That is the supervisor's failure, not the product's, and it is
the reason the capture path now calls `settle()` and refuses to shoot while the
field is up.

The real problems the critics found, in their words:

- **Pinned tabs were not drawn at all.** `pins_holder` had `no_show_all(true)`
  and `gtk_widget_show_all` does not show such a widget, so four pinned tabs
  existed in the model and appeared nowhere on screen.
- **Empty and error states did not exist.** A new tab was a white rectangle; a
  failed load was `not found` flush to the corner.
- **The field said "Search" twice**, had no keyboard navigation of its
  suggestions, and had no picked row.
- **Panels were a fixed 680x560 card** whatever they held, with no shadow on a
  white page and empty-state text at 2.29:1.
- **Dark mode was a dark sidebar on a white browser**: the web view's background
  was hardcoded white, the dark scrim was lighter than the window it covered,
  and switches were stock Adwaita blue.
- **Reading mode was pinned to the left of the window**, because it forced
  `body { margin: 0 }` and then centred in the page's own body.
- **Geometry**: traffic lights 34px down instead of 8, pins 30px instead of 34,
  row text too dark when resting and too flat when live.

## Round 2 — 7.00/10 on the facets re-scored

Two Gemini Hydrogen critics, medium thinking, on fresh round-3 screenshots
(capture fixed, pins drawn, empty state written, panels content-sized,
monochrome switches, bright lights, page background following the appearance):

- **Fidelity: 3 → 7.** Sidebar 232 against 233, lights 14px at (8,8) against
  14px at (9,9), pins 34 against 35, active pill 211x28 against 214x29, row
  stride 30 against 30-31. Remaining: a 1px black line across the top of the
  page area in the older shots, and a 20px vertical offset above the pins.
- **First impression: 3 → 7.** The silhouette reads as Search; the scrim bug is
  gone. Remaining: the same 1px line, a divider darker than the reference's
  `#ECECEC`, and an empty state that is centred but plain.

Those two remaining geometry items were fixed after round 2 and re-shot as
`council/round-4` at the reference's own window size (1500x917), which is the
size every later comparison should use: the earlier 1740x1012 shots compared a
larger window against the reference and made everything look cramped.

## What the number is made of

`council/round-1/scores.json` and `council/round-2/scores.json` hold the
per-facet numbers, the critic, and the weight. The honest summary:

| | round 1 | round 2 |
|---|---|---|
| fidelity | 3 | 7 |
| first impression | 7 (re-scored) | 7 |
| every other facet | 3-6 | not re-scored |

The mean cannot be claimed above 7.0 because nineteen facets have not been
graded since the fixes. Saying "9" here would be inventing a number.

## What the next round would do, in order

1. Re-score all twenty facets on `council/round-4` (the shots already exist).
2. Field: a picked row that keyboard and mouse agree on, and a chip that names
   the site rather than the mode.
3. Panels: a card that is a table of things rather than a box, with row hover
   and a search field in each.
4. Reading mode: re-measure the column against the page's own measure, and put
   the byline where a reader expects it.
5. Dark mode: a page-surface rule that follows the appearance without forcing
   colour on sites that have their own.
6. Memory: the 100 MB figure is the browser's own (Pss_Anon); the resident total
   is about 200 MB because GTK and WebKit's shared libraries are counted. Say
   that in the README, in those words, once.

## Evidence

- `council/round-1/*.png`, `council/round-1/dark/*.png` — the washed shots, kept
  because they are why the round scored what it did.
- `council/round-3/*.png`, `council/round-4/*.png` — after the fixes, at the
  reference's window size.
- `reference/reference-mac.png`, `reference/reference-window.png` — the Mac
  original at its own size.
- `build/stress-report.json` — 37 of 38 checks passing; the one failure is idle
  drift measured before the engine had settled, now settled in the suite.
- Critic transcripts: the sixteen reviewer reports and the four Gemini Hydrogen
  reports are in the session artifacts; their scores and quoted evidence are
  summarised here and in `scores.json`.
