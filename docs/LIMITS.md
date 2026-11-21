# What this browser does not do, and the numbers behind it

Honest limits, with the measurement that shows each one. Both tables quote the
full run of 21 November, which passed every check it made, written down from
`python3 tools/report_figures.py`. That command prints the figures out of
`build/stress-report.json`; the third column names the result each row reads.
The ranges in the prose are the claim; a table row is one sample.

Every run overwrites that file, so a table can outlive the run it copied.
`python3 tools/report_figures.py --check` is how that shows up: it compares
each figure on this page with the report on disk right now and names the ones
that disagree. That is the whole guard, and it has one limit worth stating:
while the report on disk is a run that failed, `--check` will disagree with a
page that is quoting an older passing one, and it is right to. A run that ends
with failures is not quotable, because it was measuring a browser that was
already broken. In that state the page is waiting for a run, not wrong about a
number. Figures taken by hand are marked as such.

As this is written the report on disk is that passing run, and `--check` agrees
with every figure on this page. If it ever disagrees, read which run is on disk
before editing a number: a failed run is not quotable, and a page quoting the
last good one is waiting for a run rather than wrong about it.

## Memory

One idle tab, three processes: the frame, a page, a network.

| what | measured | result in the report |
|---|---|---|
| one idle tab | 227 MB total PSS, 96 MB own | `idle one tab` |
| 20 tabs | 711 MB total PSS, 571 MB own | `20 tabs open` |
| per tab | +24.2 MB resident | `20 tabs open` |
| the difference | the shared GTK and WebKit libraries, counted once per process | |

Both numbers move. Across the runs of `tools/stress.py` on this machine the
browser's own memory for one idle tab has lain between 49.4 and 118.7 MB. The
low end came after two engine knobs were made unconditional (below); before
that the range was 62.0 to 118.7 MB. A single number would be a claim about a
machine and a page, not about the browser, so treat 49 to 119 MB as the honest
range and the row above as that run's point in it.

A 384 MB memory limit now applies to the network process and to every page
process, through `web::page_memory_pressure()`. It starts freeing caches at a
third of the limit and frees what it can at half, checking every 5 seconds.
It did not lower the memory-spray peak: the last run reached 1185.9 MB, against
1141 MB in an earlier run before the limit existed. The likely cause is live JavaScript objects,
which has not been measured.

The requested target is about 100 MB total PSS. This sample does not meet it.
The suite records `target_pss_mb: 100` and `target_met: false` alongside its
regression thresholds. Anonymous PSS is a diagnostic figure, not a substitute
for total memory.

The own-memory measure is `Pss_Anon` summed over the browser's dedicated
process group: anonymous memory, each shared page divided by the number of
processes mapping it. Group membership includes WebKit helpers that are
reparented during startup and would be missed by a parent/child walk. It is
not the whole cost: the same run reads 226.7 MB of total proportional set size,
of which 9.2 MB is shared memory (`Pss_Shmem`), and 323.0 MB of RSS. All are printed
in the report beside it.

The `20 tabs` row is the check's own name: the suite opens twenty more tabs, so
the window held 22 when it measured, which is what the check's detail line
says. The per-tab figure is the difference divided by the twenty it opened.

The resident figure is mostly the engine, not this browser: shared GTK and
WebKit libraries are counted once per process, and the suite counts the whole
tree. What the suite enforces is one idle tab under 120 MB of the browser's own
memory, under 260 MB resident, and no more than five processes, which the range
above clears.

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
about 35 MB of its own memory in the last stress run (121.7 -> 87.2 MB).

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

Add-ons can be fetched from addons.mozilla.org, which the Extensions panel
opens in a tab. A downloaded `.xpi` goes through the same installer and the
same ceiling as a file chosen by hand. The store is not a promise that an
add-on will work here: one that does its work in a background page, as most
large add-ons do, installs and is listed as not running, for the reason above.
A Firefox add-on is known by its own id, so downloading it twice or updating it
replaces the installed copy.

The ad and tracker blocker is not that extension and does not need it. It is
EasyList and EasyPrivacy compiled into three WebKit content blocker lists by
`tools/build_shield.py`: `network.json` at its cap of 14,000 rules (`MAX_NETWORK`),
with strict network option filtering so rules carrying unsupported options are
dropped rather than widened; `hide-0.json` as one global hide-element rule
carrying 2,626 selectors (26 written by hand and 2,600 taken from the lists,
which is `MAX_COSMETIC`); and `hide-sites.json` with 1,774 domain-scoped
cosmetic rules (`if-domain`). That is 15,775 WebKit rules across the three
lists (17,550 reported by `src/shield.rs`, which counts both `"url-filter"` and
`"selector"` keys). Those caps are what the lists were cut to, not counts of
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

Search-engine suggestions are off by default. When on, the text typed in the
address field goes to the chosen engine (DuckDuckGo, Google, Bing, Brave or
Startpage, https only) after a 120 ms pause, and an answer later than 0.8 s is
dropped. Nothing is sent from a private tab, the Ctrl K switcher or a keyword
shortcut, or for text that looks like an address, path, email, key or card
number, or that is over 80 characters. Known false negative: a single token of
12 or more characters that mixes letters and digits is never sent, so no
suggestion comes back for it. The rules are heuristics in `src/suggest.rs`.

Importing from another browser reads bookmarks and history from six browsers:
Firefox, Chromium, Chrome, Brave, Edge and Vivaldi (every profile). History is
capped at 4000 entries, only http and https URLs are kept, and an import only
adds. The databases are read on a worker thread, so a large import does not
freeze the window.

Pop-ups from a page with no click are blocked and offered as "Open it anyway".
`tools/stress.py` checks that no tab appears without a click (`a pop-up without
a click does not open a tab`), and that a memory spray adds at most 1500 MB
(`spray_growth_mb`, 783.5 MB in the last run). Both passed in the last run.

Video: YouTube played with sound at 720p (1280x720, 1138 frames, 0 dropped) and
1080p (1920x1080, 1065 frames, 1 dropped). dav1d is not installed on the
machine, so AV1 decoding is unverified. A real video entering fullscreen through
the page has not been re-run with the bench `fullscreen` check (`src/bench.rs`),
which only shows that the tab column and bookmark row hide and come back.

## Tested, and not

`tools/stress.py` reproduces slow, stalled, looping, 404, popup, download,
memory-spray, resize and idle states. It prints how many checks it made and how
many passed, which the table below quotes from the latest run.

It drives the browser's own actions, not the compositor's input: real mouse and
keyboard events at the window level, full screen, notifications and media
playback are checked by hand.

## The worst cases measured

| case | measured | result in the report |
|---|---|---|
| a page that allocates without limit | 1239 MB resident at its peak, 1907 MB RSS, 1073 MB own | `a memory spray is survived` |
| once it settles | 433 MB resident | `the engine survives a spray and keeps working` |
| tab switch | mean 1 ms, p95 2 ms, worst 3.5 ms over 300 switches| `tab switches stay quick` |
| a panel opening | mean 28 ms, worst 403 ms over 80| `panels open and close` |
| checks | 117 of 117 | `passed` / `failed` |
| nothing grows while nothing happens | 0.0 MB of its own over an idle minute | the detail line of `nothing grows while nothing happens` |

Every row but the last is a figure `python3 tools/report_figures.py` prints for
the run quoted above, under the label in the third column. The drift is read
from the detail line the run writes
(`208.6MB -> 208.5MB (-0.1MB), own 87.6MB -> 87.6MB (+0.0MB)`), which that
command does not print, so the row says where it comes from instead of implying
it. What the check asserts is that neither total nor own memory grew by more
than 10 MB while nothing was happening; an earlier run of it read -4.0 MB,
because the engine will give memory back while a page sits still.

The switch row is the one place the host shows through. Other runs on the same
machine measured worst cases of 67 ms and 140 ms for a similar mean, and a run
made while the machine was swapping measured 259.5 ms. The suite asserts a
mean under 60 ms and a worst under 500 ms, and records p95 so the tail is
visible instead of being averaged away. A worst case is a property of the host
as much as of the browser, which is why the mean is the assertion.

Closing tabs gives memory back, but not all at once: the last passing run went
from 582.8 MB to 220.1 MB after the tabs were closed, against a 223.8 MB baseline
before they were opened (`tabs give their memory back`). Closing every tab down
to one brought the browser down to 147.8 MB total PSS and 60.3 MB own memory
(`memory comes back down`).
The browser itself does not grow when a page sprays memory; that tab does, and
closing it is the way back. The older figures on this page for a 5,000-element
page and a closed window are gone: the suite no longer produces them, and a
number nothing measures is a number that will be wrong.

## The test count

`cargo test` runs the tests in `src/`, and `cargo build --release` prints no
warnings. The count is what the command prints; this page does not repeat it,
because it changes whenever a test is added.
