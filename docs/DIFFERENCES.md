# What is different from the Mac build

Search was written for macOS. This is the same idea on Linux, and it is not the
same program. Where the two differ, this page says how, and whether the
difference is a platform fact, a decision, or something simply missing.

## The window

The Mac window is an `NSWindow` with real traffic lights, a real titlebar, and
the system's own window shadows and corners. On Linux the browser draws its own:
a `GtkWindow` with no desktop titlebar, a zero-height client-side `HeaderBar`
kept alive only so a drag can move the window, and three coloured circles drawn
in a `GtkEventBox` and wired to close, minimise and maximise.

That is the largest fidelity decision in this build, and it is deliberate: a
desktop titlebar above a Mac-style frame reads as two windows stacked. The cost
is that window rules, snapping and the system menu come from the compositor
rather than from the header, and window shadows differ between GNOME, KDE and
whatever else is running.

## The engine

The Mac build's pages are `WKWebView`, running WebKit with Apple's own
networking. Here pages are `WebKitGTK` 4.1. Both are WebKit, but they are not the
same WebKit: the JavaScript engine, the media stack, the codecs, the
certificate store and the font stack all come from the system. A site that needs
a Chromium-only feature fails in both. A site that needs Apple-specific media
will fail only here.

Memory is the visible consequence. Safari's WebKit shares a great deal between
processes because the OS was built for it; on Linux each WebKit process maps the
same shared libraries again, which is why the resident total is about 210 MB for
one tab while the browser's own anonymous memory is about 101 MB.

## Keys, menus and the system

- `Cmd` becomes `Ctrl`. Every other modifier pair is kept.
- The Mac build has a real menu bar in the system menu. Here the menu is one
  button at the foot of the column, because there is no system menu to put it in.
- Services, Share, Handoff, iCloud tabs, Spaces and full-screen gestures are not
  ported. They are Mac surface with no Linux equivalent in this build.
- Downloads go to the XDG download directory (`~/Downloads` by default), not to
  a Mac `NSSavePanel` sheet.
- Settings, history, bookmarks, hidden elements and the session are JSON files
  under `$XDG_CONFIG_HOME/search` and `$XDG_DATA_HOME/search`, not preferences
  in `~/Library`.

## Reading mode

On the Mac this is a second view over the article. Here it is done inside the
page: `assets/reader.js` takes the article out of the document, sets it again in
the browser's own type, and remembers the original body so it can be put back.
That costs no second process and no second load, and Back does not have to be
rewritten to undo it.

The difference a reader will notice: a page that builds itself with JavaScript
after the article is extracted can behave differently under reader mode here,
and scripts that the page relies on to keep running do not keep running.

## Things this build has that the Mac build does not

- A bench socket (`$XDG_RUNTIME_DIR/search-bench.sock`) and `tools/bench.py`,
  so the browser can be driven and measured while it runs. The Mac build has a
  bench tool as well; this one is not a port of it, and its commands are its own.
- `tools/stress.py`, 48 checks, which measures memory and timing per scenario.
- `--shot`, `--size`, `--appearance`, `--fresh` and `--bench` flags, for taking
  pictures of the browser and for reproducing a failure on a clean profile.
