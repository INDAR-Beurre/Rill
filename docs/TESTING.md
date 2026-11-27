# Testing Rill

Rust unit tests, real-browser socket/stress suites, and a native GTK walkthrough.
(The internal Cargo package and binary remain named `search`; local preview
candidate binaries live under `build/`; the spaces candidate is
`build/rill-spaces-f48`.) Keep historical `1.0.2` and `F39`
results separate from current verification.

## Current preview verification

The local `build/rill-spaces-f48` candidate is an unreleased preview. F49 keeps
that filename and replaces onboarding's large outlined capsules with flat
choices and colored native icons. It also integrates the appearance/layout
customization controls. Tests use temporary profiles; generated concepts are
separate from the real screenshot gallery in `docs/screenshots/rill-f48/`.
Checksums and results for each tested binary are recorded in
[`benchmarks/f49-verification.json`](benchmarks/f49-verification.json).
The [F48 report](benchmarks/f48-verification.json) is historical.

- **Rust:** 395 passed, zero failed, two live-network tests ignored by default;
  optimized release build without compiler warnings.
- **Welcome/customization:** real pointer checks cover all three presets,
  independent toolbar actions, ordering, tab/sidebar widths, homepage controls,
  profile export/import, live welcome preview/revert, and restart persistence.
  Tests resolve current mapped controls and scroll them into view before
  clicking; artwork centers are clicked separately to catch GTK DrawingArea
  event interception. The pure-Cairo switch is tested with pointer and Space.
- **Parent integration:** 145 checks passed, including repeated real X11
  keyboard layout switches, thirty space switches, restart persistence of blank
  pins and per-space selections, private exclusion, and Default-space safety.
- **Native Wayland:** 30 checks passed on headless Sway for exclusive layouts,
  spaces, decoration layout, and GTK warnings. These checks use the bench socket;
  they do not establish native Wayland pointer or hardware-motion coverage.
- **Independent real-pointer QA:** covers icon-center clicks, tab and pin drag
  reordering, context menus, fullscreen, spaces management, the global tab cap,
  introduction actions, Settings controls, browser-menu actions, window controls,
  print-dialog cancellation, and synthetic Chromium bookmark import. A focused
  F48 control sweep passed 29 grouped checks on `63fe718b…`; that result is
  historical. Current and carried-forward results are identified by exact
  checksum in the F49 report. X11 walkthrough and stress runs require Openbox
  or another EWMH window manager for resize/maximize assertions; bare Xvfb
  does not provide those window-management behaviors.
- **Live websites/media:** Wikipedia and Rust loaded. YouTube's player advanced
  by four seconds with a decoded 854×480 frame when official GStreamer good/libav
  plugins were supplied through an isolated environment. On the unmodified
  Artix test host without those decoders, YouTube reported unsupported playback.
  This distinguishes a tested playback path from missing system dependencies.

Only X11 Openbox and headless Wayland Sway were available. Other desktops,
distributions, physical printer output, and hardware compositor smoothness
remain outside this run. Passing the local suites does not certify every
external website, extension, or production environment.

## Current GTK UI walkthrough

```sh
SEARCH_TEST_BACKEND=x11 SEARCH_UI_OUTPUT=/tmp/search-ui python3 tools/test_ui.py
python3 tools/test_ui.py --self-test   # non-GUI helper checks (output & backend policy)
```

Run against an already-built checkout: the default binary is
`target/release/search`; set `SEARCH_BINARY=target/debug/search` for a development
build. Both `tools/test_ui.py` and `tools/stress.py` share the same backend
selection rule: an explicit `SEARCH_TEST_BACKEND` (`x11` or `wayland`) always
wins; otherwise Wayland is preserved when `WAYLAND_DISPLAY` is set, and X11 is
selected automatically when `WAYLAND_DISPLAY` is unset and `DISPLAY` is set.
Pass `SEARCH_TEST_BACKEND=x11` (against X11 or `Xvfb`) for deterministic
programmatic resize assertions on tiled Wayland compositors; native Wayland runs
are separate and X11/`Xvfb` passes do not substitute for measured native
Wayland or hardware-compositor motion smoothness. The walkthrough starts the
real GTK browser with temporary
`XDG_CONFIG_HOME`, `XDG_DATA_HOME`, and `XDG_CACHE_HOME`
directories plus a dedicated `SEARCH_BENCH_SOCKET`. A local fixture server
provides actual pages; the normal user profile is not used.

Use this walkthrough for visual and behavior checks of:
- Native minimal start-page search, optional bookmark shortcuts, real cached
  favicons, and autocomplete. URL input goes into the actual GTK entry and activates
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

Screenshots, `test-ui.log`, and `manifest.json` land in `build/redesign/` by
default, or in `SEARCH_UI_OUTPUT` when set (leaving `build/redesign/` untouched).
The `shot` path captures the constructed GTK frame and WebKit page, not an HTML
recreation or simulated UI. Design F39 ran `tools/test_ui.py` cleanly
against both the source release binary (automatic X11 selection when `WAYLAND_DISPLAY`
is unset and `DISPLAY` is set) and the packaged
`build/Search-redesign-x86_64.AppImage` (zero GTK criticals, zero GTK warnings,
and zero invalid allocations, including Settings native Cairo `pill-switch` /
`engine-picker` controls, Settings filter-clear bottom scroll, Site Card `18px`
inset + `scrimclick` dismiss, Omnibox unsupported-scheme chip rejection, and
live TLS `errorfocus` initial/revisit/overlay-non-stealing checks). All seven
gallery images in `docs/screenshots/` are refreshed from the final Design F39
wide captures.

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
PyGObject — Design F38 (and `1.0.2`) validation accepted **15,775 rules across
3 lists, 0 rejected** (`hide-0.json`: 1, `hide-sites.json`: 1,774,
`network.json`: 14,000), matching `shield::state()` after the F38 rule-counting
fix in `src/shield.rs`. It
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
(`src/import.rs`). These run without a display. Design F39 Rust suite tally: **337 passed, 0 failed, 1 ignored (338 total)**
from `cargo test --release` (up from F38's 324 passed, 1 ignored; includes new
F39 IPv6/curtain, folded-controls, TLS `errorfocus`, drift-tracker, and bounded
first-paint motion regression tests). The 1 ignored test is `suggest::tests::live_duckduckgo_answers_in_the_documented_shape`,
a live-network DuckDuckGo shape check documented in `src/suggest.rs` and run
explicitly with `cargo test --release live_duckduckgo -- --ignored --nocapture`.

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
| `spaces` | named spaces and the active space |
| `spacecreate <name>` / `spacerename <id> <name>` | create or rename a space, applying the same validation as Settings |
| `spaceswitch <id>` / `spacedelete <id>` | switch spaces, or move a custom space's tabs to Default and remove it |
| `tabspace <tab-id> <space-id>` | move a tab to another space |
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
it: `one idle tab stays under the 230MB WebKitGTK floor` fails above 230 MB
total PSS; `idle one tab` fails above 120 MB of the browser's own anonymous
memory (`anon_mb`), 260 MB total PSS, or 7 processes (F38 measured 3 processes at idle and after History, while
Settings can still spawn `glycin-image-rs` + 2 `bwrap`); `tab switches stay quick` fails above a 60 ms mean
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

The archived Design F39 full run (`docs/benchmarks/stress-report-f39.json`,
**151 passed, 0 failed, 151 checks**) recorded one idle tab at **182.9 MB warm
total PSS** (185.3 MB cold), **94.5 MB own anonymous PSS**, and **441.9 MB
RSS** across **3 processes** (`search`, `WebKitWebProcess`,
`WebKitNetworkProcess`; zero `glycin`/`bwrap` at idle, after History/Bookmarks,
and while/after Settings), tab switches at **4.6 ms mean**, **13.5 ms p95**,
and **15.7 ms worst** over 300 switches, and panels at **27 ms mean**, **62 ms
p95**, and **81 ms worst** over 80 toggles (with honest post-F39 `DriftTracker`
`panel motion has no long main-loop gaps` at `open 73ms, close 50ms, 855 ticks,
gaps []`). The original ~100 MB total-PSS target remains unmet. Historical Design
F38 (`docs/benchmarks/stress-report-f38.json`, 150/150 passed, 146.1 MB PSS / 3
idle procs, 6 after Settings) and Design F37 (`docs/benchmarks/stress-report-f37.json`,
142/142 passed, 188.1 MB PSS / 6 procs) remain archived separately.
Earlier historical `1.0.2` baseline numbers (**117/117 checks**, 218.1 MB total
PSS, 94.2 MB own, 318.2 MB RSS, tab switches 1.0 ms mean / 1.7 ms p95 / 6.5 ms
worst) remain archived in `council/round-48/stress-report-before.json`.
`python3 tools/report_figures.py` prints rows from whichever run is currently
saved in `build/stress-report.json`; reading that report is not a fresh run. Two other baseline-era full runs on the same machine were recorded
by hand and are in no file here: a quiet one measured mean 1.7 ms, p95 3.4 ms
and worst 67 ms, and one made while the host was swapping measured a 259.5 ms
worst. The worst varied substantially between these older samples. The worst
case is a property of the host as much as of the browser, which
is why the assertion is on the mean and the recorded p95 is the evidence about
the tail.

## 4. Screenshot and pixel-gate harness (`tools/shots.sh`)

```sh
tools/shots.sh build/council/design-f39-light
SHOT_DARK=1 tools/shots.sh build/council/design-f39-dark
```

`tools/shots.sh` has been expanded from its original `1.0.2` 18-state harness
into the active **40-state** capture and pixel-gate suite (44 manifest entries
including binary SHA-256 stamping and 3 geometry/pixel gates) rebaselined to
the warm paper/charcoal palette in both wide (`1500×918`, rendered `1500×919`
under X11) and narrow (`900×1001`, rendered `900×1002` under X11) modes. It
captures fresh empty-profile start (`08a-start-fresh`), default chrome,
revealed toolbar (`01b-toolbar`), pins (`02-pins`, `02b-pins-live`), tabs,
omnibox (`04-omni-current`, `04-omni`, `04b-omni-picked`, `04c-omni-ranking`,
`04d-omni-tls-failed`), all six panels (including empty/filled library states,
filtered Settings, and true bottom-of-Settings scroll verified by both pixel
diff and `vadjustment` geometry), find (`06-find`, `06b-find-zero`), reading
mode sizes and find (`07-reading*`), populated start page (`08-empty`), all six
error states (`09-*`), `16-sitecard` (verified `18px` right inset + dismiss
scrim), `17-peek`, and `18-folded-column`. Note: when running headless under
`xvfb-run` on a Wayland host, pass `SHOT_BACKEND=x11` (and unset
`WAYLAND_DISPLAY`) so GTK targets Xvfb at the exact requested dimensions.

Live non-`SEARCH_BENCH` 60fps motion verification runs via
`python3 tools/live_motion.py --backend wayland-sway-gpu --out <dir>` (isolated
headless Sway on Intel Iris Xe `/dev/dri/renderD128` at 60 Hz using persistent
Wayland `zwp_virtual_keyboard_v1` + `zwlr_virtual_pointer_v1` input and
continuous `wf-recorder -D` capture + unrecorded `GdkFrameClock` telemetry) and
`python3 tools/live_motion.py --backend x11-xvfb --out <dir>` (`Xvfb + Openbox`
software X11) across all **19 cases** (6 entrances × normal/reduced, 2 exits ×
normal/reduced, and 3 mid-flight reversals: `panel_reverse`, `fold_reverse`,
`peek_reverse`), checking binary/source provenance, Peek chrome-only ROI, zero
GTK warnings/criticals, restoration to baseline, and live scrim click-outside.

### Gallery provenance

`council/round-47` and `council/round-47-dark` are the historical `1.0.2`
screenshot runs (18 states). `docs/screenshots/{light,dark,omnibox,pins,reading,history,settings}.png`
are refreshed directly from the final Design F39 wide X11 captures
(`build/council/design-f39-light/` and `build/council/design-f39-dark/`). Do not substitute simulated UI, mockups,
or unverified images.

## What is not tested

Real input events (mouse and keyboard at the compositor level): the walkthrough
uses real GTK controls but drives their actions through the socket. The
video-fullscreen hide/restore path is exercised there; real page-triggered
fullscreen, window-manager integration, notifications, media playback, and the
engine's own captchas still need manual checks. Palette and geometry checks do
not replace a manual review of legibility, focus, and pointer/keyboard behavior.
