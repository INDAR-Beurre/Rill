# What this browser does not do, and the numbers behind it

Honest limits, with the measurement that shows each one.

## Memory

| what | number | where it comes from |
|---|---|---|
| one idle tab, resident (PSS) | about 210 MB | three processes: frame, page, network |
| one idle tab, the browser's own (Pss_Anon) | about 115 MB | `tools/stress.py`, `memory()` |
| the difference | the shared GTK and WebKit libraries | counted once per process, shared with the desktop |

There is no configuration of this browser that puts three WebKit processes with
a page loaded under 100 MB resident. That is the engine's floor, not the
browser's. The 115 MB figure in the README is the browser's own memory, and it says so.

`SEARCH_LOW_MEMORY=1` turns off the DMA-BUF renderer and shares one malloc
arena for a few tens of megabytes, at the cost of the fastest painting path.
Tabs let go of their page after half an hour (Settings, on by default).

## Engine

Pages are rendered by WebKitGTK, the engine the desktop already has. A site that
needs a Chromium-only feature will not work here. Extensions do not exist.

## Tested, and not

`tools/stress.py` reproduces slow, stalled, looping, 404, popup, download,
memory-spray, resize and idle states, 38 checks, all green. It drives the
browser's own actions, not the compositor's input: real mouse and keyboard
events at the window level, full screen, notifications and media playback are
checked by hand.

## The one number that is not a claim

`cargo test` runs 10 tests. `cargo build --release` prints no warnings.
