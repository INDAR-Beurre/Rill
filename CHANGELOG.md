# Changelog

## Unreleased

- Contrast raised across secondary text and the borders that are not text, so a
  hint and a hairline are legible on both grounds instead of only on the dark
  one. The quiet grey the hints are set in went from 1.48:1 on white to 5.42:1
  (`#6A6A68` in `src/css.rs`), the muted grey from 5.10:1 to 6.48:1, the input
  border from 1.25:1 to 2.08:1 light and 1.37:1 to 3.04:1 dark, and the
  hairline from 1.23:1 to 1.51:1 light and 1.37:1 to 1.54:1 dark. Ratios
  computed with the WCAG formula from the hex values in `src/palette.rs` and
  `src/css.rs`.
- Square window corners. A radius on the frame showed on one edge and was
  clipped on the other, which read as a smudge rather than a shape.
- A pin with no favicon shows its letter, and pins wrap to the width of the
  column instead of the sixth one being cut off at the edge.
- The address field no longer repeats the host above its own text, and no longer
  slides while you type: its position comes from the window height alone, so a
  suggestion arriving in the list below cannot move it. It has a visible focus
  edge of its own.
- Settings: switches draw a track and a knob of the right size, and the chosen
  appearance chip is visibly chosen.
- One inset for the panel card, so every panel starts at the same place.
- Return in Find steps through the matches; Shift Return steps back.
- `F11` full screen, `Ctrl U` view source, and `Escape` stops a page that is
  still loading.
- An error page can be left with the mouse, and a 4xx offers to change the
  address instead of reloading one that is not there.
- The shield loads its compiled lists from the engine's own store after the
  first launch, rather than recompiling 14,000 rules every time.
- The suite asserts the numbers this documentation quotes, so a threshold that
  drifts and a page that drifts cannot disagree without a red run.

## 1.0.0

The first Linux build.

- A column of tabs with pinned pages, a live row, and a field over the page.
- WebKitGTK as the engine: no Chromium inside, about 2.8 MB of binary, stripped.
- Ad and tracker blocking as a compiled content blocker, before the request,
  built from EasyList and EasyPrivacy (the two lists uBlock Origin ships
  enabled) by `tools/build_shield.py`. None of that extension's code runs here.
- Hidden elements per site, applied before the first frame.
- Reading mode, find, downloads, bookmarks, history, settings, and a Shortcuts
  panel so the keys are discoverable.
- Light, dark, or the desktop's own.
- Extensions: content scripts, their CSS, host patterns and options pages, from
  a `.crx`, an `.xpi` or a folder, with the panel stating what will not run.
  There is no storage API, so an extension that keeps state in
  `browser.storage.local` keeps none. Read the `## Extensions` section of the
  README before expecting a Chrome extension to just work: WebKitGTK has no
  WebExtensions API, so an extension whose work lives in a background page is
  installed and marked as not running rather than quietly doing nothing.
- A bookmarks bar, hidden until there is something in it.
- A resizable window whose size and maximised state are remembered.
- *Use less memory* in Settings: one idle tab measured 62 to 66 MB of the
  browser's own memory with it on, across four runs, against 60 to 88 MB with
  the two always-on knobs and 87 to 104 MB with nothing set. The switch applies
  at the next launch.
- Two engine knobs that are now always set, because four runs each showed they
  cost nothing to keep: no DMA-BUF renderer, and two allocator arenas instead
  of one per thread. Turning compositing off on its own was measured and made
  memory worse, so that one stayed a switch.
- A bench socket and a stress suite, so the browser can be tested while it runs.

### Fixed while it was being scored

Each of these was found by a critic or by the suite, not by guessing:

- A pop-up took the screen while the tab bar still named the tab it opened
  from, so `Ctrl W` closed the page the reader was looking at.
- An error page belonged to the window rather than to its tab: a 404 in a
  background tab painted over the page being read.
- Reading mode was inert for two rounds: its script interpolated a palette
  containing a quote into JS source, so the whole file failed to parse.
  `node --check` guards it now, and the mode carries its own way out.
- A panel cut its last row in half, and sat 115px left of centre.
- Find printed 4294967295 matches, because `found-text` reports the text it
  found and `-1` until it has counted. It reads `counted-matches` now.
- Reopening a closed tab deleted the history entry it was reopened from.
- Every `Shift` shortcut was dead, because GDK reports the upper-case letter
  when Shift is held and the key was matched by its lower-case name. The key
  table is now a pure function with its own tests, and the suite drives the
  same one the keyboard does.
- The blocker did nothing on 9,565 of its 14,000 rules and then nothing at all:
  WebKit rejects any filter containing an alternation, and one bad rule fails
  the whole list. `tools/check_shield.py` now compiles the generated lists with
  the engine's own parser.
- `window.open` from a private tab opened a public tab, and a private page's
  address went into the history file.
- An extension's CSS was accepted by WebKit's user style sheet and had no
  effect at either level; it is injected at document start now, which works.
