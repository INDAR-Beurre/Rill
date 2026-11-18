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
panels.rs      bookmarks, history, downloads, settings
reader.rs      reading mode (assets/reader.js does the work)
curtain.rs     hidden elements, per site, before first paint
shield.rs      the content blocker, compiled once
store.rs       prefs and the small JSON files
files.rs       reading and writing those files
bench.rs       the socket tests drive the browser through
layer.rs       where the things over the page stand
icons.rs       the marks the UI draws for itself
gtkx.rs        signal returns
```

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

## The frame

`ui::build` makes one `GtkPaned`: the column on the left, the page on the right,
with the column's edge as the handle, so a width can be dragged. The column is a
vertical box of fixed pieces: the lights' band, the pins' grid, the rows in a
scrolled window, and the foot.

Rows are rebuilt when the set of tabs changes, not on every title change: the
browser keeps the row's widgets on the tab and updates the label in place.

Nothing in `ui.rs` knows what a tab is, and nothing in `app.rs` knows what a GTK
widget looks like beyond the handle it was given.

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

`tools/stress.py` measures PSS and Pss_Anon for the whole tree. With one tab open
and idle, the browser's own memory is about 101 MB and the resident total about
210 MB; the difference is the shared GTK and WebKit libraries, which the desktop
has in memory anyway.

`SEARCH_LOW_MEMORY=1` turns off the DMA-BUF renderer and shares one malloc
arena, which buys a few tens of megabytes at the cost of the fastest painting
path. Tabs also let go of their page after half an hour (Settings › Let tabs
sleep), which returns the page's memory to the system.
