# Testing Search

Five layers, none of them mocks.

## 0. The three suites beside the stress suite

```sh
python3 tools/extensions.py    # the extension loader, on built fixtures
python3 tools/check_shield.py  # every generated filter rule, compiled
python3 tools/compare.py       # a screenshot measured against the original
```

`tools/extensions.py` builds three extensions in a temporary folder: one with a
content script and CSS for `127.0.0.1`, one that asks only for a background
service worker and the `tabs` and `cookies` permissions, and one locked to
`example.com`. It installs all three and asserts what each one actually does:
that the marker lands, that the background-only extension is reported as not
running with a stated reason, and that the extension locked to another site
leaves the page alone. So the panel's claim about what runs here is tested
rather than described.

`tools/check_shield.py` compiles the real generated lists (`network.json`,
`hide-0.json`, `hide-sites.json`) with the engine's own parser through
PyGObject — currently 15,775 rules accepted across 3 lists, 0 rejected. It
exists because WebKit rejects a whole list for one bad rule, which is how
9,565 rules once blocked nothing at all.

`tools/compare.py` measures a screenshot against
`reference/reference-window.png`: the divider's column, the window controls'
position, the first row of ink, the ground colour. It is how arguments about a
few pixels get settled with numbers.

## 1. Unit tests

```sh
cargo test
```

The pure parts: turning what was typed into an address, host extraction, the
search engine's URL, the history's human dates, download filenames, extension
match patterns, the stylesheet's contrast pairs, the key table, VSync motion
easing curves (`src/motion.rs`), permission rules (`src/permissions.rs`), site
card security/data rules (`src/sitecard.rs`), and 6-browser import
(`src/import.rs`). These run without a display. Verified current suite:
**269 passed (1 ignored), 0 failed**.

## 2. The bench socket

```sh
SEARCH_BENCH=1 ./target/release/search &
python3 tools/bench.py ping
python3 tools/bench.py open https://example.com
python3 tools/bench.py probe
python3 tools/bench.py text
python3 tools/bench.py shot /tmp/page.png
```

A running browser listens on `$XDG_RUNTIME_DIR/search-bench.sock` and takes one
command per line, answering each with one JSON line:

| command | what it does |
|---|---|
| `ping` | version, so a script knows the browser is up |
| `probe` | every tab, the active one, the field, the panels, find, window size |
| `tabs` | the tabs, shorter than probe |
| `open <url>` | a new tab, selected, at that address |
| `select <id>` / `close <id>` / `closeall` | as their names say |
| `go <url>` | navigate the active tab |
| `peek <url>` | open a Peek card over the active tab at `<url>` |
| `peekjs <code>` | run script in the open Peek card's view, returning its value or error |
| `hide <selector>` | hide `<selector>` on the active tab's host via `curtain::pick_selector` |
| `key <name>` | any verb from `keys::SHEET` (`omni`, `newtab`, `newtabn`, `privatetab`, `closetab`, `closetabshift`, `reopen`, `switchtab`, `duplicatetab`, `foldcolumn`, `sitecard`, `reader`, `hide`, `unhide`, `settings`, `history`, `bookmarks`, `downloads`, `shortcuts`, `find`, `findnext`, `findprevious`, `fullscreen`, `escape`, …) plus `up`, `down`, `blocker`, `pin`, `closepanel`, `extensions`, `maximise`, or a tab number `1`–`9` |
| `omni <text>` | raise the field, type, press Return |
| `find <text>` | find bar up, searching |
| `filter <text>` | filter the open panel (`bookmarks`, `history`, `downloads`, …) |
| `paneltext` / `paneltree` | visible labels (up to 4,000 chars) or widget geometry inside the open panel |
| `panelgeometry` / `panelscroll <dy>` | allocated sizes and scroll range of the open panel, or scroll vertically by `<dy>` |
| `panelkey <left\|right\|tab>` / `panelfocus` | move between panel tabs or focus the panel, and report whether focus is inside it |
| `cards` | settle and report overlay dimensions and geometry/children of visible floating cards (`peek-card`, `site-card`, `toast-bar`) |
| `errorgeometry` | report error page allocation, visibility, reason and address |
| `pins` | report pins holder/grid geometry, child count and pinned tab count |
| `debug` | report omnibox visibility, allocation, window coordinates and offer count |
| `settle` | flush pending GTK resize/allocation passes and return overlay dimensions |
| `activate` | present the browser window |
| `js <code>` | run script in the active page, returning its value or the error |
| `text` | the page's title, address, length and first 20,000 characters |
| `hidden` | what is hidden on every site |
| `blocker` | toggle the ad/tracker blocker for the active site's host |
| `shield` | report compiled filter list count, finished count and total rule count |
| `extensions` / `installext <path>` | list installed extensions and `why_not`, or install an extension from `<path>` |
| `downloads` | downloads so far |
| `forget` | clear saved bookmarks and refresh the bookmark row |
| `session` | save the session immediately |
| `resize <WxH>` | resize the window |
| `fullscreen [on\|off]` | with `on` or `off`, drives video fullscreen through the same path a page's video takes; always reports `sideShown`, `bookmarksShown`, `sideWidth`, `windowFullscreen` and `flag` |
| `shot <path>` | write a PNG of the window, compositor not required |
| `orphans` | report orphan WebKit view count |
| `drift` / `reset` | report main-loop tick count and gaps > 120 ms, or clear recorded gaps |
| `quit` | close the browser |

`tools/bench.py` wraps it: `Bench.command`, `probe`, `open`, `wait_loaded`,
`wait_for`.

## 3. The stress suite

```sh
python3 tools/fixtures.py 8099 &     # a server of awkward pages
python3 tools/stress.py              # drives it, and reports
python3 tools/stress.py --quick      # about a minute
```

The server serves pages that behave like the web does: a stalled response, a
redirect chain, a redirect loop, a 404, a script that spins, a script that
sprays memory, twelve frames, pop-ups, a download, a form, a long read, eight
thousand rows, forty images.

The suite starts a browser with a throwaway profile, then works through:
startup, one idle tab, twenty tabs, three hundred tab switches, hard pages,
misbehaving pages, panels and find, reading mode, hiding (including verifying
that an untrusted page-world `searchHide` message is refused while the trusted
`hide` verb persists across reloads), downloads and pop-ups, private tab
isolation, duplicating a private tab while keeping it private, reopening a
closed tab (`Ctrl+Shift+T`), Peek card geometry/fit/escape, SiteCard
open/inset/escape, folded tab column collapse/unfold, a resize storm, closing
everything, `/dev/full` stderr resilience, tab sleeping, and an idle minute.
It measures PSS, Pss_Anon, RSS and process count for the whole tree at each
step, writes `build/stress-report.json`, and fails if the browser dies, a check
fails, or a threshold is missed.

The thresholds are the product's promises, and they are assertions in the suite
rather than prose. Each one is in `tools/stress.py`, in the check that measures
it: `idle one tab` fails above 120 MB of the browser's own memory, 260 MB
resident or five processes; `tab switches stay quick` fails above a 60 ms mean
or a 500 ms worst, and records p95; `panels open and close` fails above a 200
ms mean; `a memory spray is survived` fails above 2,600 MB; `the spray adds at
most 1500 MB over the reading before it` fails on the `spray_growth_mb` that the
run records; `memory comes back
down` fails above 260 MB after everything closes; `nothing grows while nothing
happens` fails above 10 MB of drift over the idle minute, in resident and in the
browser's own memory separately. Three more are relative to what was measured
rather than absolute, because an absolute bound that the run clears by three
times cannot catch a regression: `20 tabs open` fails above 40 MB per tab,
`tabs give their memory back` fails if closing them leaves more than 60 MB
above the baseline taken before they were opened, and `the engine survives a
spray and keeps working` fails above twice the idle tab's resident memory. Grep
the check's name to read the number: the line it sits on moves, the name does
not.

The pop-up checks (`downloads and popups`) are `a pop-up becomes a tab`, and
`a pop-up without a click does not open a tab`, which uses `/popup-auto` and
`/popup-target` from `tools/fixtures.py`. Both passed in the last run.

The measured values. The last run that passed every check, which is the one
`build/stress-report.json` holds (**117 passed, 0 failed, 117 checks**),
recorded a tab switch mean of **1.0 ms**, **p95 1.7 ms** and a **worst of
6.5 ms** over 300 switches, panels at **22 ms mean** (`22.0 ms`) and **101 ms
worst** (`100.7 ms`) over 80 toggles, and one idle tab at **218.1 MB resident**
(total PSS; 318.2 MB RSS) and **94.2 MB** of its own memory (`anon_mb`).
`python3 tools/report_figures.py` prints those rows from
`build/stress-report.json`, so check them against
whatever run is in there. Two other full runs on the same machine were recorded
by hand and are in no file here: a quiet one measured mean 1.7 ms, p95 3.4 ms
and worst 67 ms, and one made while the host was swapping measured a 259.5 ms
worst. The mean barely moved between the three; the worst moved by a factor of
four. The worst case is a property of the host as much as of the browser, which
is why the assertion is on the mean and the recorded p95 is the evidence about
the tail.

## 4. Pictures

```sh
tools/shots.sh council/latest                      # every state, light
SHOT_DARK=1 tools/shots.sh council/dark            # the same states, dark
```

Captures **18/18 states** in both light and dark modes: the default state,
pins, a column of tabs, the field (open and keyboard-picked), each of the six
panels (`settings`, `history`, `bookmarks`, `downloads`, `shortcuts`,
`extensions`), find, reading mode, an empty tab, a failed 404 page, `16-sitecard`,
`17-peek`, and `18-folded-column`, writing one PNG per state plus a manifest and
running pixel checks (`pixel dark corner`, `pixel light scrim`, `pixel panel
scrollbar`) adapted for the bar-free layout (`deskband` removed). On a desktop,
it attempts `spectacle` first when `SEARCH_SHOTS=real` and falls back to the
browser's own `shot` capture path, which draws the frame and lays the engine's
own snapshot into the page's rectangle so it works on a locked or headless
desktop and produces the same picture every time.

## What is not tested

Real input events (mouse, keyboard at the compositor level), because the suite
drives the browser's actions directly. Full-screen, notifications, media
playback and the engine's own captchas. Those are checked by hand, and this
page is the only place that says what they are.
