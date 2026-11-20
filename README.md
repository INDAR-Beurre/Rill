# Search

A small, fast, quiet web browser for Linux: a column of tabs and the page. No
toolbar, no start page, no account, no sidebar of suggestions.

![Search, with its tabs down the left and a page taking the rest of the window](docs/screenshot.png)

Search was made for macOS by [Office Commun](https://officecommun.com) and is
open source at [driceroland/Search](https://github.com/driceroland/Search).
This is a Linux build of the same idea, written from scratch in Rust against
the web engine your desktop already ships, WebKitGTK, so there is no second
copy of Chromium to download, update and keep in memory.

* about 2.8 MB of binary, stripped. `Cargo.toml` sets `strip = true`, and
  `stat -c %s target/release/search` prints the size of the build in front of you
* three processes with a tab open, as in Safari: the frame, a page, a network.
  The last run that passed every check measured **208.2 MB proportional resident memory, of which
  90.0 MB was this browser's own** (`Pss_Anon` summed over the tree); the rest is
  the shared GTK and WebKit libraries the desktop already has, plus 9.9 MB of
  shared file backing. Across runs on this machine the browser's own share has
  ranged from 49 to 119 MB. Both figures move with the page and with the host.
  Turn on Settings → *Use less memory* and the same tab measured 62 to 66 MB of
  its own memory across four runs taken before two of those knobs became
  unconditional. The suite fails a run where one
  idle tab takes more than 120 MB of the browser's own memory, more than 260 MB
  resident, or more than five processes.
* every page runs in WebKit's own process

## What it does

- **One field.** `Ctrl L` raises it over the page. Type an address and you go
  there; type words and you search. It suggests from your own history, your
  open tabs and your bookmarks, and sends nothing anywhere until you press
  Return.
- **Tabs that stay out of the way.** Tabs down the left, in a column that is
  yours to drag wider. Pin the pages you keep open all day and they shrink to a
  letter. Tabs from your last session come back, and cost nothing until you
  click one.
- **An ad and tracker blocker that runs before the page.** EasyList and
  EasyPrivacy compiled into a WebKit content blocker and enforced inside
  WebKit's own networking: nothing to render, nothing to pay for per frame. The
  compiled rule list is a JSON file you can read, and the engine's own store
  keeps it between launches instead of recompiling it every start.
- **Hide anything, for good.** `Ctrl Shift H`, click a cookie banner, a
  newsletter overlay, a rail of "related" nonsense: it goes, and it is still
  gone on that site next time, before the page has drawn a single frame.
- **Reading mode.** `Ctrl Shift R` takes the article out of the page and sets
  it again in the browser's own type.
- **A bookmarks bar**, a row of the pages you keep above the page, hidden until
  there is something in it and switchable in Settings.
- **Find, downloads, bookmarks, history, shortcuts, extensions**: each a panel,
  each one keystroke away. The shortcuts panel (`Ctrl /`) exists so the keys
  are discoverable, not folklore.
- **Extensions, from a `.crx`, an `.xpi` or a folder.** Content scripts and
  their CSS, limited to the sites they ask for, with the panel telling you what
  will and will not run here. See [Extensions](#extensions) for the honest
  version.
- **A window that remembers its size**, and maximised state, across launches.
- **Light, dark, or the desktop's own.** The frame follows. Pages follow when a
  site offers a dark page; for the ones that never do, Settings has a switch
  that gives them the browser's ground instead.
- **A private tab** with its own cookie jar, gone when it closes.

## What it does not do

On purpose: no sync, no account, no cloud, no telemetry, no second window, and
no WebExtensions API behind the extension loader. Tabs, history, bookmarks and
what you have hidden are small JSON files in your own folders and nowhere else.

Things that decide whether you can switch, stated plainly:

- **DRM video does not play.** Netflix, Spotify's web player and anything else
  that needs Widevine will refuse, because the engine ships without it. YouTube
  is fine.
- **No password manager and no autofill.** Use your desktop's clipboard or a
  password manager's own app.
- **PDFs open in the engine's own viewer**, which is basic: no annotations, no
  form saving.
- **No WebExtensions API, so no `chrome.*`.** WebKitGTK has no such layer, so
  an extension here is content scripts, their CSS and an options page, and
  nothing more; anything that lives in a background page does not run.
  [Extensions](#extensions) is the whole of it, including which extensions will
  not work.
- **No multiple windows and no tab dragging between them.** One window, tabs in
  its column.
- **No reordering tabs by dragging.** A tab sits where it was opened. The row's
  menu can pin, duplicate or close it, and pinning moves it to the pinned grid,
  but nothing in the browser moves a tab to a position you choose.
- **No print dialog.** `Ctrl P` is the page's own.

## Extensions

Install one from the panel (`Ctrl ,`, then Extensions) by typing or pasting a
path: a `.crx` from the Chrome store, an `.xpi` from the Firefox store, or an
unpacked folder. It is unpacked into `~/.local/share/search/extensions/` and
listed with its own switch.

What runs here is what the engine can do:

| | |
|---|---|
| Content scripts (`content_scripts`, `run_at`, `matches`) | run, before or after the document, only on the sites the extension asks for |
| Their style sheets (`css`, `user_scripts`) | applied, at document start |
| An options page (`options_ui`, `options_page`) | opens in a tab, if the file stays inside the extension's folder |
| `browser.storage.local` | **not available.** Without an API layer there is nowhere for it to live, so an extension that keeps its state there keeps nothing |
| A background page or service worker | **does not run.** There is no WebExtensions API in WebKitGTK |
| `chrome.tabs`, `chrome.cookies`, `webRequest`, `chrome.proxy` | **not available** |

An extension whose work is all in a background page is installed, listed, and
marked *not running here* with the reason, rather than silently doing nothing.

The ad and tracker blocker is not an extension and does not need one. It is
EasyList and EasyPrivacy, the two lists uBlock Origin ships enabled, converted
by `tools/build_shield.py` into the content blocker WebKit enforces in its
network process. The network list fills the tool's cap of 14,000 rules; the
hidden-element rule carries 2,626 selectors, 2,600 from the lists and 26
written by hand for the cookie bars and interstitials the suite tests. It is
not uBlock Origin itself: none of that extension's code runs here, so its
element picker, its dynamic filtering syntax and its scriptlets are absent.
What replaces the picker is the browser's own `Ctrl Shift H`, which hides an
element on one site and keeps it hidden; what replaces its scriptlets is
nothing. What it keeps is the part WebKit can enforce, which is the part that
runs before a request is made. `tools/check_shield.py` compiles the generated
lists with the engine's own parser and fails if a single rule is rejected,
because a list that does not parse is a list that blocks nothing.

## Build it

Needs GTK 3 and WebKitGTK 4.1 (package `webkit2gtk-4.1` on Arch,
`libwebkit2gtk-4.1-dev` on Debian and Ubuntu), Rust 1.75 or newer, and a
running Wayland or X11 session.

```sh
cargo build --release
./target/release/search
```

That is the whole build. There is no bundler, no node, no submodule. It builds
with no warnings: `cargo build --release` should say nothing at all.

### Install it

```sh
cargo install --path . --root ~/.local     # puts `search` in ~/.local/bin
mkdir -p ~/.local/share/applications
cat > ~/.local/share/applications/search.desktop <<'EOF'
[Desktop Entry]
Type=Application
Name=Search
Comment=A small, fast, quiet browser
Exec=search %u
Terminal=false
Categories=Network;WebBrowser;
MimeType=x-scheme-handler/http;x-scheme-handler/https;
EOF
xdg-mime default search.desktop x-scheme-handler/https
```

`~/.local/bin` is on `PATH` on most desktops; if it is not, add it. `xdg-mime`
makes Search the browser links open in. There is no package, no AppImage and no
Flatpak: one binary, one `.desktop` file, and GTK and WebKitGTK from your
distribution.

### What it costs next to what you have

One idle tab measured 208.2 MB proportional resident memory in the last run that passed every
check, of which 90.0 MB was this browser's own; across runs the browser's own
share has ranged from 49 to 119 MB. For scale, the same page in Firefox on this
machine was 480 MB over five processes in a reading taken by hand from
`ps_mem`. No script in this repository produces that figure, so treat it as one
afternoon's comparison rather than a benchmark. That is the whole pitch, and it
is why the engine is the one the desktop already has in memory rather than a
second copy shipped in the binary.

## Drive it, test it

```sh
python3 tools/fixtures.py 8099 &     # pages that are hard on a browser
cargo build --release
SEARCH_BENCH=1 ./target/release/search &
python3 tools/bench.py probe         # what the browser is showing
python3 tools/bench.py open https://example.com
python3 tools/bench.py text
python3 tools/stress.py              # the whole stress suite
python3 tools/extensions.py          # the extension loader, on built fixtures
python3 tools/check_shield.py        # do the generated rules compile at all
python3 tools/compare.py             # measure a screenshot against the original
```

`tools/stress.py` opens too many tabs, switches between them too fast, loads
pages that never finish, pages that eat memory, pages that redirect in a loop,
opens and closes every panel, hides things, uses find and reading mode, resizes
the window, and watches the memory the whole time. It writes
`build/stress-report.json` and exits non-zero if anything fails. The last run
that passed every check was on a 4-core desktop with 7.5 GB of RAM, as `nproc`
and `/proc/meminfo` report them, and the run prints how many checks it made.

The full list of places this build differs from the Mac original, and why, is
in [docs/DIFFERENCES.md](docs/DIFFERENCES.md).

It has already earned its keep: it caught a panel that wedged the whole UI
because the card asked for a preferred size inside an allocation pass, and a
redirect chain that hung because the curtain was synced from inside a policy
decision.

`tools/bench.py` speaks to a running browser over a Unix socket, one command
per line. It is the same idea as the Mac build's `./bench`.

## Keyboard

| | |
|---|---|
| `Ctrl L` field · `Ctrl T` new tab · `Ctrl Shift N` private tab | `Ctrl Tab` / `Ctrl Page Down` next · `Ctrl Shift Tab` / `Ctrl Page Up` previous |
| `Ctrl W` close · `Ctrl Shift T` reopen | `Alt ←` / `Alt →` back, forward · `Ctrl [ ]` the same |
| `Ctrl 1` to `Ctrl 9` jump | `Ctrl R` reload · `Ctrl Shift R` reading mode · `F5` reload · `Ctrl F5` reload past the cache |
| `Ctrl D` bookmark this page | `Ctrl F` find |
| `Ctrl Y` history · `Ctrl Shift B` bookmarks · `Ctrl J` downloads | `Ctrl ,` settings |
| `Ctrl Shift H` hide something · `Ctrl Shift U` what is hidden | `Ctrl Shift C` copy address |
| `Ctrl +` `Ctrl -` `Ctrl 0` zoom | `Ctrl /` every shortcut, listed |
| `F12` or `Ctrl Shift I` inspector · `Escape` closes what is open, else stops a loading page | `F11` full screen · `Ctrl U` view source |

## How it is put together

- **GTK 3** for the frame and the widgets, **WebKitGTK** for pages, **Rust**
  everywhere else. One module per concern in `src/`.
- `src/app.rs` is the browser: tabs, what is on screen, and every action a key,
  a click or a page can ask for. `src/web.rs` makes a page and tells the
  browser what the page is doing. `src/ui.rs` builds the widgets, `src/css.rs`
  and `src/palette.rs` make them look like this, `src/omnibox.rs`,
  `src/find.rs` and `src/panels.rs` are the things that come over the page.
- A tab's page is made when you first look at the tab. A tab restored from last
  session costs no process until you click it.
- The shield is a compiled content blocker, built from EasyList and EasyPrivacy
  (the two lists uBlock Origin ships enabled). None of that extension's code
  runs here. Hidden things are a user style sheet set before the document
  begins. Reading mode runs in the page, so it costs no second view and no
  third process.
- An extension contributes scripts to the content manager a page is built with,
  so an extension costs no separate process either.
- `src/bench.rs` is the socket tests drive the browser through;
  `tools/shots.sh` takes a picture of every state without needing a compositor.

## Why this and not another browser

If you have one window open all day with a handful of pages and you want the
window to be a column of tabs and the page, this is the whole of it. It does
not download an engine: it uses the one your desktop already has, so a release
upgrade is a `cargo build`, and starting it is one process until a page loads.
If you want a WebExtensions engine, sync or a second window, it will disappoint
you on purpose (see [What it does not do](#what-it-does-not-do)).

## Licence

MIT. See [LICENSE](LICENSE).
