# Council memo

The bar was **9/10**. It was not reached. Rounds 1 to 4 took the score from 3.98
to 8/10 on the facet the product is judged by first, and left three facets low
enough to hold the mean where it is.

## The rounds

| round | evidence | facets scored | mean |
|---|---|---|---|
| 5 | `council/round-11` | 10 | 7.16 |
| 1 | `council/round-1` (screenshots taken with the field open by mistake) | 20 | 3.98 |
| 2 | `council/round-3` (capture fixed) | 7 | 6.03 |
| 3 | `council/round-7` (real compositor captures) | 16 | 5.62 |
| 4 | `council/round-10` (real captures, flattened, at the reference's size) | 6 | 6.10 |

Round 4's mean is over six facets, not twenty, so it is not comparable with
round 1's. What is comparable:

| facet | r1 | r3 | r4 | r5 |
|---|---|---|---|---|
|---|---|---|---|
| fidelity | 3 | 7 | 8 | **9** |
| first impression | 3 | 6 | not re-scored |
| typography | 4 | 5.5 | 6.5 | 7 |
| tab row | 6 | 7 | not re-scored |
| pins | 3 | 6 | not re-scored |
| empty and error | 3 | 6 | not re-scored |
| dark mode | 5 | 5 | 5.5 | 7 |
| panels | 4.5 | 4 | 5 | 7 |
| field | 4 | 5 | 4 | **8** |
| code craft | 4 | 4 | **3.5** |

## What each round actually fixed

**Round 1 to 2** (the capture that lied): the shot harness captured every state
with the address field open, so a 74% white scrim washed the window and the
visual critics scored a screen nobody would see. Fixed by settling the over-page
layers before every shot, and by clearing a round's directory before writing it.
Also fixed here: pinned tabs were in the model and absent from the screen
(`no_show_all` is not shown by `show_all`), there was no empty state, panels were
a fixed 680x560 box with 2.3:1 empty text, dark mode was a dark sidebar on a
white browser, and reading mode sat left of the window.

**Round 2 to 3**: an own error page for failed loads and HTTP 400/404/5xx
(reason in words, address, Try again), one global error became per tab, panels
measure their content, stock GTK control skin removed (buttons, combo, menu,
switches), dark-mode switch knobs visible again, the pin strip drawn, the field's
scrim scoped to the page instead of the window, a picked suggestion row that
Up/Down actually move.

**Round 3 to 4**: screenshots became real window captures flattened onto the
window's own size (a 2000x1200 canvas had shifted every measurement by 250px and
invented a "scrollbar"), the divider became one hairline at `#ECECEC`, traffic
lights moved to the reference's height and size (15px at y=31), pins to 36px at
the reference's offset and grey, rows to 30px with the reference's 8px inset, the
live row to bold ink, the chosen appearance chip to a real state (a CSS typo,
`buttonbutton.chosen`, had hidden it), and the dark scrim to one that darkens.

## Why it is not 9

Three facets are holding it down, and all three are the same kind of problem:
the frame is now close, the parts that need *interaction* are not.

1. **Field, 4/10.** The card and the list are clean and the keys work, but the
   field is centred on the window instead of on the page, its chip only appears
   when the active tab has an address (so it is missing in the state the shots
   capture), the picked row has no accent beyond a wash, and the suggestion
   kinds are a faint 11px label. The ability to photograph it is new; the design
   of it is not done.
2. **Panels, 5/10.** The nav strip, the elevation and the empty copy are fixed.
   Rows only act through their right-hand pill, not on a click of the row
   itself; History cuts a row at the card's edge; the card still carries slack
   under a short list.
3. **Code craft, 3.5/10.** The base layer is small and honest (`layer.rs`,
   `files.rs`, the pure functions), and everything above it repeats the same
   three-line GTK idiom twenty times. Four `unused()` stubs were removed this
   round, ten icon variants and five palette fields are still unused, and the
   one place that must be careful, `shield.rs`'s raw FFI, has been rewritten to
   hold the store until the compile answers but still has no test.

A fourth is close behind: **dark mode, 5.5**. The page area is white whenever
the site does not paint its own dark surface. Forcing colour on sites that do not
ask for it is the wrong fix; the honest options are a page-surface rule the user
can turn off, or nothing and a README that says so.

## Round 5, and what it changed

Fidelity reached the bar at 9. First impression and the field reached 8. The
rest are recorded in `council/round-5/scores.json`, and the mean over the ten
facets scored is 7.16. After those scores were taken, the fixes their GAPS named
landed: the field opens filled with the address and selected, panel filters are
wired, Ctrl Shift R is reading mode, Ctrl / lists every key, a Shortcuts panel
exists, panel rows act on the primary button only, the panel scrim respects the
column, session writes are debounced (own memory 115 MB to 101 MB), the shield's
async slot is no longer freed while a callback may hold it, and the build prints
zero warnings. Those are in `council/round-12` and are unscored.

## What the next round would do

1. Field: centre on the page pane, always show the chip once a tab has an
   address, give the picked row a left accent and a keycap, group suggestion
   kinds with a quiet header.
2. Panels: rows act on click; a scrolled body that clips at a row boundary; a
   search field in Bookmarks and History.
3. Code craft: delete the unused icons and palette fields, collapse the repeated
   GTK idiom into three helpers, and put the shield's FFI behind a test that
   compiles a rule list.
4. Re-score all twenty facets on `council/round-10`, so the mean is comparable
   with round 1 again.

## Evidence

- `council/round-10/*.png` and `council/round-10/dark/*.png` — the current
  states, real captures, flattened, 1500x917, the reference's own window size.
- `reference/reference-window.png` — the Mac window at that same size.
- `build/stress-report.json` — 38 of 38 checks, one idle tab at 210.6 MB
  resident, 115.3 MB of it the browser's own.
- `cargo test` — 10 tests, all passing.
- `council/MEMO.md` and the per-round `scores.json` files carry the numbers the
  critics gave, facet by facet.

## Round 7 (code, not pictures)

Two critics, both 6/10, both found things worth having:

- **A pop-up took the screen.** Every `window.open` and Duplicate Tab went through
  `navigate`, which always made the new view the visible child. The tab bar
  still highlighted the old tab, so <kbd>Ctrl</kbd>+<kbd>W</kbd> closed the tab
  the user was not looking at. The view is now made visible only if its tab is
  the active one, and the suite compares `visibleChild` against `active`.
- **An error belonged to the window, not the tab.** A 404 in a background tab
  painted over the page being read, and a clean finish anywhere dismissed an
  unrelated error. Errors are keyed by tab and painted only for the active one.
- **Reopening a closed tab deleted history.** `reopen_closed` removed the entry
  instead of reading it.
- **A panel cut its last row in half.** The card asked the scroller how tall it
  wanted to be; a scroller answers with the space it was given. It now asks the
  list inside it, so the panel is exactly as tall as its rows.
- **Panels sat 115px left of centre.** The card was centred on the window while
  the page it covers starts after the column.
- **Find printed 4294967295 matches.** `found-text` reports the text found, not
  the total, and answers `-1` until counted. The bar reads `counted-matches`
  now, and says "no matches" when there are none.

Round 7 facet scores: browser loop 6.0 (was 4.0 in round 3), field 8.0,
panels 7.0. Not a pass; the loop is the facet to push next.
