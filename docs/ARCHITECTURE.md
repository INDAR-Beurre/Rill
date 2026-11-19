# How Search is put together

A browser is a small surface with a lot underneath. This is the map.

```
main.rs        flags, the environment, the window, and the loop
app.rs         the browser: tabs, the active tab, and every action
web.rs         one WebKit view per tab, and what its signals mean
ui.rs          the widgets: the column, the rows, the pins, the lights
css.rs         one stylesheet, built from the palette
palette.rs     every colour, as a light/dark pair
omnibox.rs     the field over the page
find.rs        the find bar
panels.rs      bookmarks, history, downloads, settings, shortcuts, extensions
reader.rs      reading mode (assets/reader.js does the work)
curtain.rs     hidden elements, per site, before first paint
shield.rs      the content blocker, compiled once from the generated lists
extensions.rs  content scripts and CSS, limited to the sites they ask for
keys.rs        every shortcut: what a key means, and what it does
js.rs          running a script in a page, and hearing back
store.rs       prefs and the small JSON files
files.rs       reading and writing those files
bench.rs       the socket tests drive the browser through
layer.rs       where the things over the page stand
icons.rs       the marks the UI draws for itself
gtkx.rs        signal returns
shield_assets.rs  the generated filter lists, included at build time
```

`src/shield_assets.rs` is generated, not written: `tools/build_shield.py` turns
EasyList and EasyPrivacy into the content blocker's JSON and emits a `FILTERS`
constant of `include_str!`. Regenerate it, do not edit it. The output is a
network list at the tool's cap of 14,000 rules, and one hide-element rule
carrying 2,626 selectors: the 26 hand-written cookie-bar and interstitial
selectors, then 2,600 taken from the lists, which is the cosmetic cap.

`shield.rs` compiles each list once and hands the result to the engine's own
filter store, keyed by the hash of the list's bytes. A later launch loads the
compiled filter back instead of recompiling 14,000 rules: the key changes when
the list changes, so a new EasyList pull cannot be answered with last month's
rules.

## One tab

`app::Tab` holds the identity: id, url, title, pinned, private, and the widgets
for its row and pin. The `WebView` is not there. It is made by `web::view_for`
the first time the tab is selected, added to a `GtkStack` under the name
`tab-<id>`, and removed when the tab closes. A tab restored from the session
file therefore costs no web process until it is looked at, which is most of why
opening with twenty tabs is still instant.

The view reports itself through signals that `web::attach` wires: title, uri,
load state, navigation state, a page asking for a window, a page asking to
close. Each one calls a method on the browser, never the other way around.

The window's own geometry lives in the preferences with the rest:
`window_size` and `window_maximised` are written when the window is closed and
read when it is made. A maximised window records the size it would return to
rather than the size of the screen, so unmaximising lands where it was.

## Extensions

`extensions::install` reads a manifest, copies the folder into the data
directory, and records whether the extension can do anything here. What it can
do is bounded by the engine: content scripts, their style sheets, an options
page, and nothing that needs a `chrome.*` API. `extensions::capability` is the
function that decides, and it names the reason in the panel rather than
reporting a bare "does not run": a background-only extension is listed as not
running because there is no WebExtensions API to start it, and an extension that
asks for `tabs`, `cookies`, `webRequest`, `history`, `downloads`, `bookmarks`,
`webNavigation`, `proxy` or `management` is told which of those it will not get.
There is no storage API behind `browser.storage.local`, so an extension that
keeps its state there finds nothing kept when it reads. `tools/extensions.py`
builds three extensions to check that the answer matches what the engine does.

Scripts are contributed to the `UserContentManager` a view is built with, so a
page with an extension in it is still one process. The manager is cached on the
pair (shielded, extension generation), and installing or removing an extension
bumps the generation and rebuilds every view.

An extension's CSS does not go through `UserStyleSheet`: measured on this engine
a sheet added that way is accepted and has no effect at either
`UserStyleLevel::User` or `Author`. It is injected at document start as a
`<style>` element instead, which does work.

## The frame

`ui::build` makes one `GtkPaned`: the column on the left, the page on the right,
with the column's edge as the handle, so a width can be dragged. The column is a
vertical box of fixed pieces: the lights' band, the pins' grid, the rows in a
scrolled window, and the foot.

Rows are rebuilt when the set of tabs changes, not on every title change: the
browser keeps the row's widgets on the tab and updates the label in place.

The bookmarks bar is a row of chips in the page's column, above the stack and
below the band. It is hidden when nothing is kept and shown again when
something is, and Settings has a switch for it; both live in
`app::render_bmarks`, which is why a bookmark taken with `Ctrl D` appears
without rebuilding the column.

Nothing in `ui.rs` knows what a tab is. `app.rs` is not the mirror image of
that: it is 2,531 lines and it does touch GTK, deliberately, in one place.
`app::capture` and the `draw_at`, `opacity_of` and `draw_into` helpers under it
walk the real widget tree with cairo, because a screenshot of a window with no
compositor has to draw the widgets that were actually built, at the offsets they
were actually given. That is the exception, and it is the only one: the rest of
`app.rs` reaches the widgets through the handles `ui::build` returned, and the
only other classes it names are the GDK event types its click handlers
downcast. Grep `fn capture` to see the shape of it.

Measured with `wc -l src/*.rs` at this commit, 11,195 lines in `src`: `app.rs`
2,531, `extensions.rs` 1,685, `panels.rs` 929, `shield.rs` 717, `omnibox.rs`
696, `bench.rs` 681, `ui.rs` 582, `keys.rs` 549, `web.rs` 544, `store.rs` 484,
`curtain.rs` 351, `palette.rs` 280, `files.rs` 276, `css.rs` 242, `main.rs`
218, `icons.rs` 179, `find.rs` 149, `layer.rs` 39, `reader.rs` 27, `js.rs` 17,
`gtkx.rs` 11, `shield_assets.rs` 8. The numbers move with every commit; the
command does not.

## Things over the page

`layer::Layer` is a `GtkFixed` that knows how to place a child for a window of a
given size. The field, the find bar and the panels each own one, are added to the
root overlay, and are laid out when the window changes size. This is why the
scrim covers the window and the field lands in the middle, and why the same
layout is reproducible in a screenshot without a compositor.

## Looking without a compositor

`app::capture` draws the frame into a cairo surface, asks WebKit for a snapshot
of the visible page, lays that into the page's rectangle, and then draws the
over-page layers on top. `tools/shots.sh` drives a browser through every state
and calls it, which is how the council's screenshot evidence is made on a
machine whose screen is locked.

## Memory

Three processes with one tab open: the frame, a web process, a network process.
The frame is GTK and the browser's own widgets; the other two belong to WebKit,
as in any browser on this engine.

`tools/stress.py` measures PSS, Pss_Anon, Pss_Shmem, Private_Dirty and RSS for the
whole tree. With one tab open and idle the browser's own memory — `Pss_Anon`
summed over the three processes — measured between 49 and 119 MB, and 159.0 MB
resident in the last run that passed every check. That run also counted 9.9 MB
of shared backing and 253.3 MB RSS. The gap between own and resident is the
shared GTK and WebKit libraries, which the desktop has in memory anyway; the
shared backing is named separately so the flattering number cannot hide it.

`WEBKIT_DISABLE_DMABUF_RENDERER=1` and `MALLOC_ARENA_MAX=2` are always set,
before GTK starts: measured over four runs each, they took one idle tab from
87-104 MB of the browser's own memory down to 60-88 MB, and nothing about the
page got worse. They also moved the resident figure a long way: the same idle
tab that measured 212.8 MB resident before them measured 159.0 MB after. `SEARCH_LOW_MEMORY=1` adds `WEBKIT_DISABLE_COMPOSITING_MODE=1`,
which brought the same four runs to 62-66 MB and is the steadiest of the three
settings; it is left to the user because it is also the one that changes how a
page animates. Settings → *Use less memory* sets that flag in the preferences,
and like the variable it has to be read before the engine starts, so it applies
at the next launch. Tabs also let go of
their page after half an hour (Settings → *Let tabs sleep*), which returns the
page's memory to the system.

The run that produced 49.4 MB own and 159.0 MB resident is the one quoted on
`docs/LIMITS.md`; `python3 tools/report_figures.py` prints its figures from
`build/stress-report.json`. A number written into this file would be a number
about one afternoon.
