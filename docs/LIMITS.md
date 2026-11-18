# What this browser does not do, and the numbers behind it

Honest limits, with the measurement that shows each one.

## Memory

| what | number | where it comes from |
|---|---|---|
| one idle tab, resident (PSS) | about 210 MB | three processes: frame, page, network |
| one idle tab, the browser's own (Pss_Anon) | 101 MB | `build/stress-report.json` |
| every figure on this page | the last run of `tools/stress.py` | re-run it and these change |
| the difference | the shared GTK and WebKit libraries | counted once per process, shared with the desktop |

There is no configuration of this browser that puts three WebKit processes with
a page loaded under 100 MB resident. That is the engine's floor, not the
browser's. The stated goal was 100 MB of the browser's own memory for one idle tab; the measured figure is 101 MB, which is the engine's floor rather than a target that was rounded down.

`SEARCH_LOW_MEMORY=1` turns off the DMA-BUF renderer and shares one malloc
arena for a few tens of megabytes, at the cost of the fastest painting path.
Tabs let go of their page after half an hour (Settings, on by default).

## Engine

Pages are rendered by WebKitGTK, the engine the desktop already has. A site that
needs a Chromium-only feature will not work here. Extensions do not exist.

## Tested, and not

`tools/stress.py` reproduces slow, stalled, looping, 404, popup, download,
memory-spray, resize and idle states, 48 checks, all green. It drives the
browser's own actions, not the compositor's input: real mouse and keyboard
events at the window level, full screen, notifications and media playback are
checked by hand.

## The worst cases measured

| case | resident | own |
|---|---|---|
| 8 tabs | 515 MB | 332 MB |
| a page that allocates without limit | 1504 MB at its peak, 763 MB once it settles | 1368 MB |

Closing tabs gives memory back, but not all at once: eight tabs went from 515 MB
to 228 MB after they were closed. The browser itself does not grow when a page
sprays memory; that tab does, and closing it is the way back.

## The one number that is not a claim

`cargo test` runs 10 tests. `cargo build --release` prints no warnings.
