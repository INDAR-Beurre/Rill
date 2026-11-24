# Testing Search

Rust unit tests, real-browser socket/stress suites, and a native GTK walkthrough.
Keep historical `1.0.2` results separate from verification of the local,
unreleased UI makeover.

## Current GTK UI walkthrough

```sh
python3 tools/test_ui.py
```

Run against an already-built checkout: the default binary is
`target/release/search`; set `SEARCH_BINARY=target/debug/search` for a development
build. The walkthrough uses a working Wayland session and starts the real GTK
browser with temporary `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, and `XDG_CACHE_HOME`
directories plus a dedicated `SEARCH_BENCH_SOCKET`. A local fixture server
provides actual pages; the normal user profile is not used.

Use this walkthrough for visual and behavior checks of:
- Native start-page empty states, clearly labeled curated links, and populated
  bookmarks/recent pages. URL input goes into the actual GTK entry and activates
  its navigation path; the resulting WebKit address is checked, not simulated.
- Live light/dark switching and screenshot palette checks on the real rendered
  PNGs, rather than the old layout's hardcoded pixel coordinates.
- Wide, medium, and compact window sizes, allocated control bounds, start-page
  and settings reflow, scrolling, and sidebar folding/unfolding.
- Normal/private data separation: private start pages hide normal bookmarks and
  history, and submitting a URL keeps the private tab private. Engine-level
  cookie/storage isolation remains covered by the stress suite.
- All six panels, settings navigation and lower-section scrolling, the omnibox,
  pinned pages, history, and reading-mode captures.
- Toolbar actions and native navigation overflow at compact widths, plus the
  video-fullscreen hide/restore path and error-page presentation.

The `chrome`, `uitree`, `starttext`, `start`, `theme`, `uiclick`, `menuentries`,
and `menupick` bench verbs inspect or exercise actual GTK widgets. Geometry is
checked against the allocated window size, not against the earlier design.
`start` fills and activates the entry; `uiclick` emits the real button action.
These are widget-level checks, not compositor-level input injection.

Screenshots, `test-ui.log`, and `manifest.json` land in `build/redesign/`.
The `shot` path captures the constructed GTK frame and WebKit page, not an HTML
recreation or simulated UI. Final walkthrough verification and replacement of
the seven gallery images are pending; the coverage described here is not a
claim that a fresh final run has passed.

## 0. The three suites beside the stress suite

```sh
python3 tools/extensions.py    # the extension loader, on built fixtures
python3 tools/check_shield.py  # every generated filter rule, compiled
python3 tools/compare.py       # legacy fidelity measurement, not redesign validation
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
PyGObject — `1.0.2` validation accepted 15,775 rules across 3 lists, 0 rejected. It
exists because WebKit rejects a whole list for one bad rule, which is how
9,565 rules once blocked nothing at all.

`tools/compare.py` is a legacy fidelity measurement against
`reference/reference-window.png`: the divider's column, the window controls'
position, the first row of ink, the ground colour. Those reference coordinates
belong to the older interface and are not acceptance criteria for the makeover.

## 1. Unit tests

```sh
cargo test
```

The pure parts: turning what was typed into an address, host extraction, the
search engine's URL, the history's human dates, download filenames, extension
match patterns, the stylesheet's contrast pairs, the key table, VSync motion
easing curves (`src/motion.rs`), permission rules (`src/permissions.rs`), site
card security/data rules (`src/sitecard.rs`), and 6-browser import
(`src/import.rs`). These run without a display. Current Rust suite tally:
**303 passed, 1 ignored (304 total)**, from `cargo test --release` on 22 November
(round 76, log `build/test-r76-known-fixes.log`).

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
| `chrome` / `uitree` | live toolbar/sidebar state and control bounds, or the actual visible GTK widget tree |
| `starttext` / `start <text>` | start-page labels, or fill and activate its real GTK URL/search entry |
| `theme <light\|dark\|system>` | switch the running browser's appearance |
| `uiclick <name>` | activate a named native toolbar/sidebar button (`address`, `back`, `forward`, `reload`, `more`, `settings`, etc.) |
| `menuentries` / `menupick <label>` | inspect the open native overflow menu, or activate a sensitive item by its label |
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
spray and keeps working` fails if total PSS after the sprayed tab closes is more
than 60 MB above the reading taken just before the spray. Grep
the check's name to read the number: the line it sits on moves, the name does
not.

A round-76 motion probe added `motioncfg` and `drift` checks around a Settings
open. It confirms that benchmark mode suppresses animation
(`motioncfg.animations = false`) and that the Settings open no longer stalls
the main loop past 120 ms. The first probe measured a 138 ms gap; keeping the
Settings body in a permanent holder removed it. The probe also records measured
WCAG contrast ratios and the type ramp, so those claims are numbers rather than
prose.

The pop-up checks (`downloads and popups`) are `a pop-up becomes a tab`, and
`a pop-up without a click does not open a tab`, which uses `/popup-auto` and
`/popup-target` from `tools/fixtures.py`. Both passed in the historical `1.0.2`
baseline run, not a new redesign run.

The historical `1.0.2` baseline (**117 passed, 0 failed, 117 checks**), saved in
`build/stress-report.json` at the time,
recorded a tab switch mean of **1.0 ms**, **p95 1.7 ms** and a **worst of
6.5 ms** over 300 switches, panels at **22 ms mean** (`22.0 ms`) and **101 ms
worst** (`100.7 ms`) over 80 toggles, and one idle tab at **218.1 MB total PSS**
(318.2 MB RSS), including **94.2 MB anonymous PSS** (`anon_mb`).
These are historical memory/performance measurements, not measurements of the
local UI makeover. `python3 tools/report_figures.py` prints rows from whichever
run is currently saved in `build/stress-report.json`; reading that report is not
a fresh run. Two other baseline-era full runs on the same machine were recorded
by hand and are in no file here: a quiet one measured mean 1.7 ms, p95 3.4 ms
and worst 67 ms, and one made while the host was swapping measured a 259.5 ms
worst. The worst varied substantially between these older samples. The worst
case is a property of the host as much as of the browser, which
is why the assertion is on the mean and the recorded p95 is the evidence about
the tail.

## 4. Legacy pictures (v1.0.2)

```sh
tools/shots.sh council/legacy-1.0.2-light
SHOT_DARK=1 tools/shots.sh council/legacy-1.0.2-dark
```

For `1.0.2`, this older harness captured **18/18 states** in both light and dark
modes: the default state,
pins, a column of tabs, the field (open and keyboard-picked), each of the six
panels (`settings`, `history`, `bookmarks`, `downloads`, `shortcuts`,
`extensions`), find, reading mode, an empty tab, a failed 404 page, `16-sitecard`,
`17-peek`, and `18-folded-column`, with a PNG per state and a manifest. Its
hardcoded pixel checks (`pixel dark corner`, `pixel light scrim`, `pixel panel
scrollbar`) passed for that older bar-free layout. They target its palette and
geometry, not the redesign, and must not be reported as current UI passes.
The harness can attempt `spectacle` with `SEARCH_SHOTS=real` and otherwise uses
the browser's `shot` path; the capture mechanism does not make old assertions
valid for the new interface.

### Gallery provenance

`council/round-47` and `council/round-47-dark` are the historical `1.0.2`
screenshot runs. The gallery refresh replaces
`docs/screenshots/{light,dark,omnibox,pins,reading,history,settings}.png` with
actual final GTK captures from `tools/test_ui.py`, retaining its log and
manifest as provenance. That replacement is pending. Do not substitute
simulated UI, mockups, or unverified images, and do not cite the older pixel
checks as verification of the new captures.

## What is not tested

Real input events (mouse and keyboard at the compositor level): the walkthrough
uses real GTK controls but drives their actions through the socket. The
video-fullscreen hide/restore path is exercised there; real page-triggered
fullscreen, window-manager integration, notifications, media playback, and the
engine's own captchas still need manual checks. Palette and geometry checks do
not replace a manual review of legibility, focus, and pointer/keyboard behavior.
