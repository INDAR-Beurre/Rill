# Changelog

## 1.0.0

The first Linux build.

- A column of tabs with pinned pages, a live row, and a field over the page.
- WebKitGTK as the engine: no Chromium inside, about 850 KB of binary.
- Ad and tracker blocking as a compiled content blocker, before the request.
- Hidden elements per site, applied before the first frame.
- Reading mode, find, downloads, bookmarks, history, settings, and a Shortcuts
  panel so the keys are discoverable.
- Light, dark, or the desktop's own.
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
