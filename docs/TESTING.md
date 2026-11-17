# Testing Search

Three layers, none of them mocks.

## 1. Unit tests

```sh
cargo test
```

The pure parts: turning what was typed into an address, host extraction, the
search engine's URL, the history's human dates, download filenames. These run
without a display.

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
| `key <name>` | any of the browser's own actions: `omni`, `newtab`, `closetab`, `pin`, `reader`, `hide`, `settings`, `history`, `maximise`, … |
| `omni <text>` | raise the field, type, press Return |
| `find <text>` | find bar up, searching |
| `js <code>` | run script in the active page, returning its value or the error |
| `text` | the page's title, address, length and first 400 characters |
| `hidden` | what is hidden on every site |
| `downloads` | downloads so far |
| `resize <WxH>` | resize the window |
| `shot <path>` | write a PNG of the window, compositor not required |
| `escape` | put away the field, find and any panel |
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
misbehaving pages, panels and find, reading mode, hiding, downloads and pop-ups,
a resize storm, closing everything, and an idle minute. It measures PSS,
Pss_Anon, RSS and process count for the whole tree at each step, writes
`build/stress-report.json`, and fails if the browser dies, a check fails, or a
threshold is missed.

The thresholds are the product's promises: a tab switch under 120 ms, panels
under 200 ms, one idle tab under 260 MB resident, no growth over an idle minute.

## 4. Pictures

```sh
tools/shots.sh council/latest          # every state, as PNGs
tools/shots.sh council/latest          # with SHOT_DARK=1 for dark
```

Drives a browser through the default state, pins, a column of tabs, the field,
each panel, find, reading mode, an empty tab and a failed page, and writes one
PNG per state plus a manifest. The capture path draws the frame and lays the
engine's own snapshot into the page's rectangle, so it works on a locked or
headless desktop and produces the same picture every time.

## What is not tested

Real input events (mouse, keyboard at the compositor level), because the suite
drives the browser's actions directly. Full-screen, notifications, media
playback and the engine's own captchas. Those are checked by hand, and the
README says so.
