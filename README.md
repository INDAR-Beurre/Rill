# Search

A small, fast, quiet web browser for Linux: a column of tabs and the page.
No toolbar, no start page, no account, no sidebar of suggestions.

![Search, with its tabs down the left and a page taking the rest of the window](docs/screenshot.png)

Search was made for macOS by [Office Commun](https://officecommun.com) and is
open source at [driceroland/Search](https://github.com/driceroland/Search). This
is a Linux build of the same idea, written from scratch in Rust against the web
engine your desktop already ships — WebKitGTK — so there is no second copy of
Chromium to download, update and keep in memory.

* about 850 KB of binary
* three processes with a tab open, as in Safari: the frame, a page, a network.
  101 MB of that is the browser's own memory and 210 MB is resident in total,
  the difference being the shared GTK and WebKit libraries the desktop already
  has. Both numbers are measured by `tools/stress.py`, not estimated
* every page runs in WebKit's own process

## What it does

- **One field.** `Ctrl L` raises it over the page. Type an address and you go
  there; type words and you search. It suggests from your own history, your open
  tabs and your bookmarks, and sends nothing anywhere until you press Return.
- **Tabs that stay out of the way.** Tabs down the left, in a column that is
  yours to drag wider. Pin the pages you keep open all day and they shrink to a
  letter. Tabs from your last session come back, and cost nothing until you
  click one.
- **An ad and tracker blocker that runs before the page.** A WebKit content
  blocker compiled once at launch and enforced inside WebKit's own networking:
  nothing to render, nothing to pay for per frame. The rule list is a JSON file
  you can read and edit.
- **Hide anything, for good.** `Ctrl Shift H`, click a cookie banner, a
  newsletter overlay, a rail of "related" nonsense: it goes, and it is still
  gone on that site next time, before the page has drawn a single frame.
- **Reading mode.** `Ctrl Shift R` takes the article out of the page and sets it
  again in the browser's own type.
- **Find, downloads, bookmarks, history, shortcuts** — each a panel, each one
  keystroke away. The shortcuts panel (`Ctrl /`) exists so the keys are
  discoverable, not folklore.
- **Light, dark, or the desktop's own.** The frame follows. Pages follow when a
  site offers a dark page; for the ones that never do, Settings has a switch
  that gives them the browser's ground instead.
- **A private tab** with its own cookie jar, gone when it closes.

## What it does not do

On purpose: no sync, no account, no cloud, no telemetry, no extension engine, no
second window. Tabs, history, bookmarks and what you have hidden are small JSON
files in your own folders and nowhere else.

Things that decide whether you can switch, stated plainly:

- **DRM video does not play.** Netflix, Spotify's web player and anything else
  that needs Widevine will refuse, because the engine ships without it. YouTube
  is fine.
- **No password manager and no autofill.** Use your desktop's clipboard or a
  password manager's own app.
- **PDFs open in the engine's own viewer**, which is basic: no annotations, no
  form saving.
- **No extensions at all**, not even an engine to refuse them with.
- **No multiple windows and no tab dragging between them.** One window, tabs in
  its column.
- **No print dialog.** `Ctrl P` is the page's own.

## Build it

Needs GTK 3 and WebKitGTK 4.1 (package `webkit2gtk-4.1` on Arch, `libwebkit2gtk-4.1-dev`
on Debian and Ubuntu), Rust 1.75 or newer, and a running Wayland or X11 session.

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

One idle tab is 210 MB resident, of which 101 MB is this browser's own. The same
tab in Firefox on this machine is 480 MB over five processes, and in Chromium
based browsers it is more. That is the whole pitch, and it is why the engine is
the one the desktop already has in memory rather than a second copy shipped in
the binary.

## Drive it, test it

```sh
python3 tools/fixtures.py 8099 &     # pages that are hard on a browser
cargo build --release
SEARCH_BENCH=1 ./target/release/search &
python3 tools/bench.py probe         # what the browser is showing
python3 tools/bench.py open https://example.com
python3 tools/bench.py text
python3 tools/stress.py              # the whole stress suite, ~4 minutes
```

`tools/stress.py` opens too many tabs, switches between them too fast, loads
pages that never finish, pages that eat memory, pages that redirect in a loop,
opens and closes every panel, hides things, uses find and reading mode, resizes
the window, and watches the memory the whole time. It writes
`build/stress-report.json` and exits non-zero if anything fails. It currently
reports 56 of 56 checks passing on a 4-core Linux desktop with 8 GB of RAM.

The full list of places this build differs from the Mac original, and why, is in
[docs/DIFFERENCES.md](docs/DIFFERENCES.md).

It has already earned its keep: it caught a panel that wedged the whole UI
because the card asked for a preferred size inside an allocation pass, and a
redirect chain that hung because the curtain was synced from inside a policy
decision.

`tools/bench.py` speaks to a running browser over a Unix socket, one command per
line. It is the same idea as the Mac build's `./bench`.

## Keyboard

| | |
|---|---|
| `Ctrl L` field · `Ctrl T` new tab · `Ctrl W` close · `Ctrl Shift T` reopen | `Ctrl [ ]` back, forward · `Ctrl Tab` next, previous |
| `Ctrl 1`–`Ctrl 9` jump | `Ctrl R` reload · `Ctrl Shift R` reading mode |
| `Ctrl D` bookmark this page | `Ctrl F` find |
| `Ctrl Y` history · `Ctrl Shift B` bookmarks · `Ctrl J` downloads | `Ctrl ,` settings |
| `Ctrl Shift H` hide something · `Ctrl Shift U` what is hidden | `Ctrl Shift C` copy address |
| `Ctrl +` `Ctrl -` `Ctrl 0` zoom | `Ctrl /` every shortcut, listed |
| `Ctrl Shift I` inspector | |

## How it is put together

- **GTK 3** for the frame and the widgets, **WebKitGTK** for pages, **Rust**
  everywhere else. One module per concern in `src/`.
- `src/app.rs` is the browser: tabs, what is on screen, and every action a key,
  a click or a page can ask for. `src/web.rs` makes a page and tells the browser
  what the page is doing. `src/ui.rs` builds the widgets, `src/css.rs` and
  `src/palette.rs` make them look like this, `src/omnibox.rs`, `src/find.rs` and
  `src/panels.rs` are the things that come over the page.
- A tab's page is made when you first look at the tab. A tab restored from last
  session costs no process until you click it.
- The shield is a compiled content blocker. Hidden things are a user style sheet
  set before the document begins. Reading mode runs in the page, so it costs no
  second view and no third process.
- `src/bench.rs` is the socket tests drive the browser through; `tools/shots.sh`
  takes a picture of every state without needing a compositor.

## Why this and not another browser

If you have one window open all day with a handful of pages and you want the
window to be a column of tabs and the page, this is the whole of it. It does not
download an engine: it uses the one your desktop already has, so a release
upgrade is a `cargo build`, and starting it is one process until a page loads.
If you want extensions, sync or a second window, it will disappoint you on
purpose (see [What it does not do](#what-it-does-not-do)).

## Licence

MIT. See [LICENSE](LICENSE).
