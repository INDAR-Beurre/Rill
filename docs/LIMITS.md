# What this browser does not do, and the numbers behind it

Honest limits, with the measurement that shows each one. The current archived
full-suite figures below come from the Design F39 full run (26 November),
archived at `docs/benchmarks/stress-report-f39.json` (and synced to
`build/stress-report.json`; historical F38 and F37 remain preserved at
`docs/benchmarks/stress-report-f38.json` and `docs/benchmarks/stress-report-f37.json`).
That run passed all 151 checks (151/151). One idle tab used 182.9 MB total PSS
warm (185.3 MB cold), 94.5 MB own (anonymous) PSS, and 441.9 MB RSS across 3
processes against the rebaselined WebKitGTK floor of 230 MB. The original
~100 MB total-PSS request was an anonymous-memory figure mistaken for a total;
it remains **unmet** and is not hidden here. The third
column names the result each row reads. The ranges in the prose are the claim;
a table row is one sample.

A run that fails a check still measured what it measured. A memory failure in
particular is the number this page exists to report, so failed runs are
quoted, with the failure stated next to the figure. Older figures are marked
with the run they came from instead of being mixed into the current tables.

Every run overwrites `build/stress-report.json`, so figures here also archive
their baseline snapshot (currently `docs/benchmarks/stress-report-f39.json`,
with historical `docs/benchmarks/stress-report-f38.json`,
`docs/benchmarks/stress-report-f37.json`, and earlier snapshots under
`council/round-N/`), which do not change.
`python3 tools/report_figures.py --check` compares the top table and
`docs/DIFFERENCES.md` with whichever report is on disk right now, and compares
cited tables against their archived JSON file. If they disagree, check which
run is on disk before you change a number. Figures taken by hand are marked as
such.

## Memory

One idle tab needs three core processes (the browser window, a WebKit
page process, and a WebKit network process). In the archived Design F39 full
run (`docs/benchmarks/stress-report-f39.json`, 151/151 passed),
one idle tab measured **3 processes** — and replacing Settings' `GtkSwitch` and
`GtkComboBoxText` with native Cairo `pill-switch` and `engine-picker` controls
kept the process tree at **3 processes** even while Settings is open and after
closing Settings (zero `glycin-image-rs`, zero `glycin-svg`, zero `bwrap` in
both source and AppImage audits). The run met the 230 MB WebKitGTK floor at
**182.9 MB** warm total PSS; the original ~100 MB total-PSS
request remains **unmet** by about 83 MB.

| what | measured | result in the report |
|---|---|---|
| one idle tab | 182.9 MB total PSS, 94.5 MB own (warm reading; 185.3 MB cold PSS, 441.9 MB RSS, 3 processes) | `idle one tab` (passes), `one idle tab stays under the 230MB WebKitGTK floor` (passes) |
| 20 tabs | 598.0 MB total PSS, 468.1 MB own (2616.8 MB RSS, 23 processes) | `20 tabs open` |
| per tab | +20.4 MB total PSS | `20 tabs open` |
| the difference | shared GTK and WebKit libraries plus file-backed mappings, counted proportionally in PSS and once per process in RSS | |

The same F39 one-tab reading, process by process (`per_process` in
`docs/benchmarks/stress-report-f39.json`):

| process | total PSS | own (anon) | shared files | RSS |
|---|---|---|---|---|
| `search` (window and UI) | 81.4 MB | 42.9 MB | 31.5 MB | 240.0 MB |
| `WebKitWebProcess` (page) | 71.2 MB | 30.5 MB | 37.2 MB | 127.7 MB |
| `WebKitNetworkProcess` (network) | 30.2 MB | 21.1 MB | 9.1 MB | 74.2 MB |

File-backed PSS across the three processes comes to 77.8 MB
(31.5 + 37.2 + 9.1),
plus 94.5 MB anonymous PSS and 10.5 MB shared
memory (`Pss_Shmem`). On this evidence, cache and engine settings alone are
unlikely to bring total PSS near 100 MB. The low-memory profile in `src/web.rs`
(page cache off, document-viewer cache model, no spellcheck) has not been
re-measured against the F39 baseline, so no saving is claimed for it here.

Historically (round 49), switching the window icon from gdk-pixbuf to Cairo's
PNG reader removed a `glycin-image-rs` + 2 `bwrap` helper trio and took one
idle tab from 6 processes / 225.9 MB down to 3 processes / 218.5 MB total PSS
(`council/round-49/stress-report-quick-perproc.json` before,
`stress-report-quick-cairoicon.json` after). In Design F37, loading
`edit-find-symbolic` through GTK/gdk-pixbuf in panel filter fields reintroduced
`glycin-svg` + 2 `bwrap` (6 processes, 188.1 MB PSS). In Design F38, replacing
the named GTK filter icon with Cairo `Icon::Search` eliminated `glycin-svg` at
idle and after History/Bookmarks (3 processes, 146.1 MB PSS), while Settings'
`GtkComboBoxText`/`GtkSwitch` still spawned `glycin-image-rs` + 2 `bwrap` (6
processes after Settings). In Design F39, replacing `GtkSwitch` and
`GtkComboBoxText` with native Cairo `pill-switch` (`GtkToggleButton`) and
`engine-picker` (`GtkMenuButton` + `GtkPopover`) eliminated that remaining
helper spawn too: the integrated F39 process audit (`baseline/f39-process-audit.json`)
stays at **3 processes** (`search`, `WebKitWebProcess`, `WebKitNetworkProcess`,
zero `glycin`, zero `bwrap`) across idle, History, Bookmarks, Settings open,
and after Settings closed on both the source binary (`174.0–174.7 MB` PSS) and
the packaged AppImage (`206.1–206.7 MB` PSS).

Both numbers move. Across the runs of `tools/stress.py` on this machine the
browser's own memory for one idle tab has lain between 49.4 and 118.7 MB. The
low end came after two engine knobs were made unconditional (below); before
that the range was 62.0 to 118.7 MB. A single number would be a claim about a
machine and a page, not about the browser, so treat 49 to 119 MB as the honest
range and the row above as that run's point in it.

A 384 MB memory limit now applies to the network process and to every page
process, through `web::page_memory_pressure()`. It starts freeing caches at a
third of the limit and frees what it can at half, checking every 5 seconds.
It did not lower the memory-spray peak: the archived F39 full run reached
1295.6 MB total PSS / 2200.8 MB RSS / 1143.6 MB own
(`docs/benchmarks/stress-report-f39.json`; archived F38 reached 1173.3 MB PSS,
archived F37 reached 1293.7 MB PSS, and earlier round-76 quick run reached
1159.3 MB, against 1141 MB before the limit existed). The likely cause is live
JavaScript objects, which has not been measured.

The original request was about 100 MB total PSS. Neither historical baselines
nor F37 (188.1 MB), F38 (146.1 MB), nor F39 (182.9 MB warm total PSS) meet it.
Rounds 62-75 measured 216-219 MB warm for one idle tab with three processes, so
`tools/stress.py` records `target_pss_mb: 230` and `target_met: true` as the
rebaselined WebKitGTK floor, while this page keeps the original ~100 MB request
explicitly unmet.
This is a re-baselining decision, not a claim that the original number was
reached. Anonymous PSS (`anon_mb`) is a diagnostic figure, not a substitute for
total PSS (`pss_mb`) or RSS (`rss_mb`).

The own-memory measure is `Pss_Anon` summed over the browser's dedicated
process group: anonymous memory, each shared page divided by the number of
processes mapping it. Group membership includes WebKit helpers that are
reparented during startup and would be missed by a parent/child walk. It is
not the whole cost: the archived F39 run reads 182.9 MB of total proportional
set size (`pss_mb`), of which 10.5 MB is shared memory (`Pss_Shmem`), 94.5 MB
is anonymous own memory (`anon_mb`), 97.5 MB is private dirty, and 441.9 MB
is summed RSS (`rss_mb`). All are printed in the report beside it.

The `20 tabs` row is the check's own name: the suite opens twenty more tabs
from the one-tab state, so the window held 21 tabs when F39 measured, which is
what the check's detail line says. The per-tab figure is the difference divided
by the twenty it opened.

The total PSS and RSS figures are dominated by the engine and GTK/WebKit shared
mappings, and the suite counts the whole process tree. What `tools/stress.py`
currently enforces is one idle tab under 120 MB of own anonymous memory
(`anon_mb`), under 230 MB total PSS (`one idle tab stays under the 230MB
WebKitGTK floor`), under 260 MB total PSS and at most 7 processes in `idle one
tab` (F39 measured 3 processes at idle, after History/Bookmarks, and while/after
opening Settings).

Two of WebKit's knobs are set by default before the engine starts, because
four runs of one idle tab each showed they cost nothing to keep: disabling the
DMA-BUF renderer (`WEBKIT_DISABLE_DMABUF_RENDERER=1`) and capping the allocator
at two arenas (`MALLOC_ARENA_MAX=2`) took the browser's own memory from
87.0/96.9/102.7/104.0 MB down to 60.4/75.1/75.2/87.8 MB. The DMA-BUF renderer
stays disabled by default for memory and stability unless opted in via
`SEARCH_ENABLE_DMABUF=1` (or `WEBKIT_DISABLE_DMABUF_RENDERER=0`), while GPU
compositing (`HardwareAccelerationPolicy`) remains enabled. Page buffers that
would have gone to the compositor through DMA-BUF stay ordinary memory, and
ordinary memory is what this browser is trying to spend less of.

`SEARCH_LOW_MEMORY=1`, or Settings → *Use less memory*, adds the third knob:
no compositing path at all (`WEBKIT_DISABLE_COMPOSITING_MODE=1`, overriding any
DMA-BUF opt-in). With all three the same four runs read 61.8/62.2/64.5/66.4 MB
— the smallest and the steadiest of the three settings, and the one that changes
how a page animates, which is why it is the only one left to the user. Turning
compositing off *on its own*, without the other two, was measured and made
things worse: 77.6/78.2/80.2/88.1 MB. The switch is read before the engine
starts, so it applies at the next launch.

Tabs let go of their page after ten minutes without being looked at (Settings,
on by default). A tab that is playing sound is never put to sleep, so music,
podcasts and calls in a background tab keep going. A sleeping tab gave back
36.4 MB of its own memory and 40.6 MB of total PSS in the archived F39 full run
(120.9 -> 84.5 MB own, 209.8 -> 169.2 MB total PSS, 545.9 -> 421.3 MB RSS,
`docs/benchmarks/stress-report-f39.json`). Global 10-minute sleep with audio
exemption is implemented; per-site keep-awake overrides are not implemented.

## Engine

Pages are rendered by WebKitGTK, the engine the desktop already has. A site
that needs a Chromium-only feature will not work here.

Extensions exist up to a hard ceiling: content scripts, their style sheets,
host patterns and an options page. There is no storage API either, so an
extension that keeps its state in `browser.storage.local` starts each page with
nothing kept. WebKitGTK has no WebExtensions API, so an extension that does its
work in a background page or a service worker is installed, listed, and marked
as not running rather than silently doing nothing. No extension can read your
tabs, your cookies or your history. `tools/extensions.py` is the test of that
claim, not a description of it.

Add-ons can be searched or fetched from `addons.mozilla.org` via the Add-ons &
Extensions panel (user-initiated HTTPS only, with strict redirect and byte-cap
guards). A downloaded `.xpi` goes through the same permission-confirmed
installer and the same engine ceiling as a local file. Catalog badges estimate
capability and are not certified support: an add-on that relies on background
pages, storage, `webRequestBlocking`, or `nativeMessaging` (including password
managers such as Bitwarden, 1Password, and KeePassXC-Browser) does not work
here. See [docs/EXTENSIONS.md](EXTENSIONS.md) for the 110 real-package F44
verification results (`docs/benchmarks/real-extensions-f44-summary.json`).

The ad and tracker blocker is not that extension and does not need it. It is
EasyList and EasyPrivacy compiled into three WebKit content blocker lists by
`tools/build_shield.py`: `network.json` at its cap of 14,000 rules (`MAX_NETWORK`),
with strict network option filtering so rules carrying unsupported options are
dropped rather than widened; `hide-0.json` as one global hide-element rule
carrying 2,626 selectors (26 written by hand and 2,600 taken from the lists,
which is `MAX_COSMETIC`); and `hide-sites.json` with 1,774 domain-scoped
cosmetic rules (`if-domain`). That is 15,775 WebKit rule objects across the three
lists, matching both `tools/check_shield.py` and `src/shield.rs`'s JSON array
counter (fixed in Design F38 from the earlier 17,550 key-substring double-count). Those caps are what the lists were cut to, not counts of
everything they hold. It blocks what a content blocker can block. It has no
element picker, no per-site dynamic filtering beyond the one switch this
browser offers, and no scriptlet injection, because none of those are
expressible in the format the engine enforces.

A certificate that has expired, is not yet valid, names another host or comes
from an authority the system does not trust stops the page. The error page says
which of those it was, and it offers only "Try again", never a way through.
That was checked against expired, self-signed, wrong.host and untrusted-root on
badssl.com. Revocation is not checked: WebKitGTK does not ask whether a
certificate has been revoked, so revoked.badssl.com opens. Nothing in this
browser claims otherwise.

A page can ask for notifications, location, the camera, the microphone and
clipboard read access (`Kind::Clipboard`). The browser answers each with its
own card, Allow or Block, and remembers the answer per site until it is cleared
in Settings. A private tab asks every time and remembers nothing. A request
from a tab that is not on screen is refused, since there is nobody to ask.
Device info, media key systems, pointer lock, website data access and XR are
refused without asking. The engine's own prompt is never shown. HTTP sign-in
gets the browser's own form, and the password is never stored: the engine is
told not to keep it and no save offer is made. Normal tabs persist cookies to
`cookies.sqlite` (alongside shared site storage and HSTS), while private tabs
use an ephemeral data manager and write nothing to disk. Notifications go
through the engine's own path to the desktop; the checks were against the
installed library and the session bus, not a live notification.

A page's alerts, confirms, prompts and leave-page questions are answered in a
card over its tab, headed by the page's host, so a page cannot pass itself off
as the browser. Escape cancels and Enter confirms. A tab that closes or leaves
its page answers Cancel. A page that opens three dialogs in ten seconds is
offered "Stop this page asking", which silences it until the next load. A page
asking for a file gets the desktop's own file chooser. None of these have been
clicked through on a display.

The engine's page process needs GStreamer's `autoaudiosink`, which comes from
the autodetect plugin in gst-plugins-good. The AppImage carries it, along with
`libgstaudiofx.so`, and `tools/appimage.sh` refuses to build without them. A
build run from source on a system without that plugin loses its page process on
any site that sets up audio, GitHub among them. The tab then shows "The page
crashed." and offers to try again. The fix is to install the distribution's
gst-plugins-good. On the machine this was measured on, the system build crashed
on every GitHub load, while the AppImage stayed up on GitHub for 30 seconds
with no page-process crash.

Search-engine suggestions are enabled by default for new profiles (with opt-out
in Settings; existing saved preferences are respected). When on, plain search
queries typed in the address field or start page go to the chosen engine
(Startpage default, Ecosia, DuckDuckGo, Google, Bing or Brave, https only) after
a 120 ms pause, and an answer later than 1.0 s is dropped. Nothing is sent from
a private tab, the Ctrl K switcher or a keyword shortcut, or for text that looks
like an address, path, email, key or card number, or that is over 80 characters.
Known false negative: a single token of 8 or more characters that mixes letters
and digits is never sent, so no suggestion comes back for it. The rules are
heuristics in `src/suggest.rs`.

Importing from another browser reads bookmarks and history from six browsers:
Firefox, Chromium, Chrome, Brave, Edge and Vivaldi (every profile). History is
capped at 4000 entries, only http and https URLs are kept, and an import only
adds. The databases are read on a worker thread, so a large import does not
freeze the window.

Pop-ups from a page with no click are blocked and offered as "Open it anyway".
`tools/stress.py` checks that no tab appears without a click (`a pop-up without
a click does not open a tab`), and that a memory spray adds at most 1500 MB
(`spray_growth_mb`, 622.5 MB in the round-50 quick run saved as
`council/round-50/stress-report-quick.json`; earlier runs read 771.5 to
796.7 MB, so treat this as one sample in a wide range). Both passed in that run.

Video: YouTube played with sound at 720p (1280x720, 1138 frames, 0 dropped) and
1080p (1920x1080, 1065 frames, 1 dropped). dav1d is not installed on the
machine, so AV1 decoding is unverified. A real video entering fullscreen through
the page has not been re-run with the bench `fullscreen` check (`src/bench.rs`),
which only shows that the tab column and bookmark row hide and come back.

## Tested, and not

`tools/stress.py` reproduces slow, stalled, looping, 404, popup, download,
memory-spray, resize and idle states. It prints how many checks it made and how
many passed.

The worst-case table below comes from the archived Design F39 full run
(`docs/benchmarks/stress-report-f39.json`), which ran all 151 checks (including
300 tab switches, 80 panel toggles, 20-tab open/close, memory spray, live
WebKit private-persistence/curtain/isolated-reader regressions, delayed
private-hide race check, sleep, and
a 60-second idle drift sample) and passed 151 of 151. A comment above the table
names that archived full run, and `python3 tools/report_figures.py --check`
verifies the table's rows against that saved JSON file (so a later `--quick`
run on disk does not overwrite the full-run reference). Historical Design F38
(`docs/benchmarks/stress-report-f38.json`, 150/150), Design F37
(`docs/benchmarks/stress-report-f37.json`, 142/142), and 1.0.2 full-run figures
(`council/round-48/stress-report-before.json`, 117/117) remain preserved.

It drives the browser's own actions, not the compositor's input: real mouse and
keyboard events at the window level, full screen, notifications and media
playback are checked by hand (with live Wayland GPU + X11 window-level 19-case
motion and live folded controls additionally captured in F39).

## The worst cases measured

<!-- report: docs/benchmarks/stress-report-f39.json -->

| case | measured | result in the report |
|---|---|---|
| a page that allocates without limit | 1296 MB total PSS at its peak, 2201 MB RSS, 1144 MB own | `a memory spray is survived` |
| once it settles | 416 MB total PSS | `the engine survives a spray and keeps working` |
| tab switch | mean 4.6 ms, p95 13.5 ms, worst 15.7 ms over 300 switches | `tab switches stay quick` |
| a panel opening | mean 27 ms, p95 62 ms, worst 81 ms over 80 | `panels open and close` |
| checks | 151 of 151 | `passed` / `failed` |
| nothing grows while nothing happens | -0.2 MB of its own over an idle minute (-0.1 MB total PSS) | the detail line of `nothing grows while nothing happens` |

Every row but the last is a figure `python3 tools/report_figures.py` prints for
the archived F39 run quoted above, under the label in the third column. The
drift is read from the detail line the run writes
(`175.0MB -> 174.9MB (-0.1MB), own 87.9MB -> 87.7MB (-0.2MB)`). What the check
asserts is that neither total PSS nor own anonymous memory grew by more than
10 MB while nothing was happening; an earlier run read -4.0 MB because the
engine returns memory while a page sits still.

The switch row is the one place the host shows through. Earlier runs on the
same machine measured worst cases of 67 ms and 140 ms for a similar mean, and a
run made while the machine was swapping measured 259.5 ms. The suite asserts a
mean under 60 ms and a worst under 500 ms, and records p95 so the tail is
visible instead of being averaged away. A worst case is a property of the host
as much as of the browser, which is why the mean is the assertion.

Closing tabs gives memory back, but not all at once: the archived F39 full run
went from 598.0 MB down to 186.0 MB total PSS after closing 20 tabs, against a
189.5 MB baseline before they were opened (`tabs give their memory back`).
Closing every tab down to one brought the browser down to 106.8 MB total PSS,
59.0 MB own anonymous memory, and 302.7 MB RSS across 2 processes
(`memory comes back down`).
The browser itself does not grow when a page sprays memory; that tab does, and
closing it is the way back. The older figures on this page for a 5,000-element
page and a closed window are gone: the suite no longer produces them, and a
number nothing measures is a number that will be wrong.

## The test count

`cargo test` runs the tests in `src/`, and `cargo build --release` prints no
warnings. The count is what the command prints; this page does not repeat it,
because it changes whenever a test is added.
