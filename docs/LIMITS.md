# What this browser does not do, and the numbers behind it

Honest limits, with the measurement that shows each one. Both tables quote the
full run of 19 November, which passed every check it made, written down from
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
| one idle tab | 182 MB total PSS, 56 MB own | `idle one tab` |
| 20 tabs | 498 MB total PSS, 357 MB own | `20 tabs open` |
| per tab | +15.8 MB resident | `20 tabs open` |
| the difference | the shared GTK and WebKit libraries, counted once per process | |

Both numbers move. Across the runs of `tools/stress.py` on this machine the
browser's own memory for one idle tab has lain between 49.4 and 118.7 MB. The
low end came after two engine knobs were made unconditional (below); before
that the range was 62.0 to 118.7 MB. A single number would be a claim about a
machine and a page, not about the browser, so treat 49 to 119 MB as the honest
range and the row above as that run's point in it.

The requested target is about 100 MB total PSS. This sample does not meet it.
The suite records `target_pss_mb: 100` and `target_met: false` alongside its
regression thresholds. Anonymous PSS is a diagnostic figure, not a substitute
for total memory.

The own-memory measure is `Pss_Anon` summed over the browser's dedicated
process group: anonymous memory, each shared page divided by the number of
processes mapping it. Group membership includes WebKit helpers that are
reparented during startup and would be missed by a parent/child walk. It is
not the whole cost: the same run reads 182.2 MB of total proportional set size,
of which 9.9 MB is shared file backing, and 279.8 MB of RSS. All are printed
in the report beside it.

The `20 tabs` row is the check's own name: the suite opens twenty more tabs, so
the window held 22 when it measured, which is what the check's detail line
says. The per-tab figure is the difference divided by the twenty it opened.

The resident figure is mostly the engine, not this browser: shared GTK and
WebKit libraries are counted once per process, and the suite counts the whole
tree. What the suite enforces is one idle tab under 120 MB of the browser's own
memory, under 260 MB resident, and no more than five processes, which the range
above clears.

Two of WebKit's knobs are now always set, before the engine starts, because
four runs of one idle tab each showed they cost nothing to keep: turning off the
DMA-BUF renderer and capping the allocator at two arenas took the browser's own
memory from 87.0/96.9/102.7/104.0 MB down to 60.4/75.1/75.2/87.8 MB. Page
buffers that would have gone to the compositor through the GPU stay ordinary
memory, and ordinary memory is what this browser is trying to spend less of.

`SEARCH_LOW_MEMORY=1`, or Settings → *Use less memory*, adds the third knob:
no compositing path at all. With all three the same four runs read
61.8/62.2/64.5/66.4 MB — the smallest and the steadiest of the three settings,
and the one that changes how a page animates, which is why it is the only one
left to the user. Turning compositing off *on its own*, without the other two,
was measured and made things worse: 77.6/78.2/80.2/88.1 MB. The switch is read
before the engine starts, so it applies at the next launch.

Tabs let go of their page after half an hour (Settings, on by default).

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

The ad and tracker blocker is not that extension and does not need it. It is
EasyList and EasyPrivacy compiled into a WebKit content blocker by
`tools/build_shield.py`: a network list at its cap of 14,000 rules, and one
hide-element rule carrying 2,626 selectors (26 written by hand, 2,600 taken
from the lists, which is `MAX_COSMETIC`). Those two caps are what the lists
were cut to, not counts of everything they hold. It blocks what a content
blocker can block. It has no element picker, no per-site dynamic filtering
beyond the one switch this browser offers, and no scriptlet injection, because
none of those are expressible in the format the engine enforces.

## Tested, and not

`tools/stress.py` reproduces slow, stalled, looping, 404, popup, download,
memory-spray, resize and idle states. It prints how many checks it made and how
many passed, so neither number is written down here where it could go stale.

It drives the browser's own actions, not the compositor's input: real mouse and
keyboard events at the window level, full screen, notifications and media
playback are checked by hand.

## The worst cases measured

| case | measured | result in the report |
|---|---|---|
| a page that allocates without limit | 942 MB resident at its peak, 1594 MB RSS, 776 MB own | `a memory spray is survived` |
| once it settles | 336 MB resident | `the engine survives a spray and keeps working` |
| tab switch | mean 4 ms, p95 17 ms, worst 222 ms over 300 switches| `tab switches stay quick` |
| a panel opening | mean 117 ms, worst 1498 ms over 80| `panels open and close` |
| nothing grows while nothing happens | +0.0 MB of its own over an idle minute | the detail line of `nothing grows while nothing happens` |

Every row but the last is a figure `python3 tools/report_figures.py` prints for
the run quoted above, under the label in the third column. The drift is read
from the detail line the run writes
(`203.5MB -> 202.7MB (-0.8MB), own 83.6MB -> 83.6MB (+0.0MB)`), which that
command does not print, so the row says where it comes from instead of implying
it. What the check asserts is that neither total nor own memory grew by more
than 10 MB while nothing was happening; an earlier run of it read -4.0 MB,
because the engine will give memory back while a page sits still.

The switch row is the one place the host shows through. The quoted run measured
a mean of 1.8 ms and a worst of 68 ms. Earlier runs on the same machine measured
worst cases of 67 ms and 140 ms for the same mean, and a run made while the
machine was swapping measured 259.5 ms. The suite asserts a
mean under 60 ms and a worst under 500 ms, and records p95 so the tail is
visible instead of being averaged away. A worst case is a property of the host
as much as of the browser, which is why the mean is the assertion.

Closing tabs gives memory back, but not all at once: the quoted run went from
525 MB to 211 MB after the tabs were closed, against a 211 MB baseline before
they were opened (`tabs give their memory back`).
The browser itself does not grow when a page sprays memory; that tab does, and
closing it is the way back. The older figures on this page for a 5,000-element
page and a closed window are gone: the suite no longer produces them, and a
number nothing measures is a number that will be wrong.

## The test count

`cargo test` runs the tests in `src/`, and `cargo build --release` prints no
warnings. The count is what the command prints; this page does not repeat it,
because it changes whenever a test is added.
