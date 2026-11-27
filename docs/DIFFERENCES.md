# What is different from macOS Search

Rill began as an independent Linux implementation of macOS Search. It is the
same core idea on Linux, and it is not the same program. Where the two differ,
this page says how, and whether the difference is a platform fact, a decision,
or something simply missing.

## The window

The Mac window is an `NSWindow` with real traffic lights, a real titlebar, and
the system's own window shadows and corners. On Linux, Rill negotiates window
decorations cleanly via **Settings → Window controls** (`System`, `Browser`, or
`Hidden`): in `System` mode on X11 desktops with an EWMH window manager, the
desktop draws its single native titlebar with zero duplicate browser controls;
in `Browser` mode (or Wayland sessions without server-side decorations), Rill
draws compact pill close/minimise/maximise buttons in the top tab strip
honoring `gtk-decoration-layout`.

The current Linux interface uses a top tab strip with an optional vertical tab
sidebar (`Ctrl S`) and a mouse-first navigation toolbar (visible by default for
new profiles, individually customizable in Settings or via right-click, and
toggleable with `Ctrl Shift S`). The native GTK start page renders a cached
8×8 Bayer ordered-dither horizon with customizable bookmark shortcuts (`Icons and
names`, `Icons only`, or `Icons, names & URL`). Video fullscreen hides chrome
and restores it cleanly on exit.

Panels use a navigation rail and ruled content sections. On smaller windows,
the rail becomes compact icon navigation and settings controls reflow. Light
and dark colors are intentional paper/charcoal palettes rather than the Mac
build's neutral-grayscale tokens.

## Motion and animations

Where the Mac build leans on Core Animation / AppKit, this build uses
`src/motion.rs`, a small VSync-synced GTK3 frame-clock animator built on
`WidgetExt::add_tick_callback` and `gdk::FrameClock` timestamps with cubic and
quintic easing curves. It drives the floating 3px load-progress bar (smoothly
interpolating progress and fading out on completion), smooth tab-column
fold/unfold (`Ctrl S`, sliding the `GtkPaned` divider while cross-fading body
and strip), and overlay transitions for the omnibox, panels, find bar, Peek
cards, site card, permission prompts, and bottom toasts. Under `SEARCH_BENCH`
or when `gtk-enable-animations` is false, transitions finish synchronously in a
single step with no frame delay.

## The engine

The Mac build's pages are `WKWebView`, running WebKit with Apple's own
networking. Here pages are `WebKitGTK` 4.1. Both are WebKit, but they are not the
same WebKit: the JavaScript engine, the media stack, the codecs, the
certificate store and the font stack all come from the system. A site that needs
a Chromium-only feature fails in both. A site that needs Apple-specific media
will fail only here.

Memory is the visible consequence. Safari's WebKit shares a great deal between
processes because the OS was built for it; on Linux each WebKit process maps the
same shared libraries again. That is why for one idle tab proportional resident memory was 182.9 MB total PSS in the archived F39 full run
(`docs/benchmarks/stress-report-f39.json`: 182.9 MB warm / 185.3 MB cold total PSS, 94.5 MB own anonymous PSS, 441.9 MB RSS, 3 processes across idle, History/Bookmarks, and Settings open/close with zero `glycin`/`bwrap`; passes the rebaselined 230 MB WebKitGTK floor while still missing the original ~100 MB request), while the browser's own anonymous memory is 49 to 119 MB depending on
the page and the host.

## Keys, menus and the system

- `Cmd` becomes `Ctrl`. Every other modifier pair is kept.
- The Mac build has a real menu bar in the system menu. Here a toolbar menu
  exposes all browser actions, and the column footer offers direct Settings,
  Library, History, and Downloads access.
- Services, Share, Handoff, iCloud tabs, Spaces, full-screen gestures,
  cross-app video picture-in-picture, built-in system keychain passwords /
  passkeys, and a signed macOS-style self-updater are not ported (platform gaps
  or deliberate scope decisions; see `docs/LIMITS.md` and
  `baseline/feature-parity.md`).
- Split View is represented by `Shift` + click Peek cards (a floating second
  page card with `Keep` / `Close`) rather than persistent side-by-side split
  panes, and tab sleep is global (10-minute timeout with audio exemption)
  without per-site keep-awake overrides.
- Downloads go to the XDG download directory (`~/Downloads` by default), not to
  a Mac `NSSavePanel` sheet.
- Settings, history, bookmarks, hidden elements, permission grants and the
  session are JSON files under `$XDG_CONFIG_HOME/search` and
  `$XDG_DATA_HOME/search`, not preferences in `~/Library`.
- Ordinary tabs share a persistent `WebContext` (`src/web.rs`) backed by
  `$XDG_DATA_HOME/search` and `$XDG_CACHE_HOME/search`, storing persistent
  cookies in `cookies.sqlite`. Private tabs use an ephemeral
  `WebsiteDataManager` so cookies, storage and history never reach disk. Site
  cookies and storage for a single host can be cleared from the site card
  (`Ctrl I`).
- Permissions (`src/permissions.rs`) prompt per host in an inline overlay card
  for notifications, location, camera, microphone and clipboard read requests
  (`WebKitClipboardPermissionRequest`), saving decisions per host in
  `permissions.json` for ordinary tabs and asking every time in private tabs.
  Device info, media key systems (DRM), pointer lock, website data access and
  XR are refused silently.
- Importing (`src/import.rs`) brings bookmarks and history (up to 4,000
  `http`/`https` entries, copied and read on a worker thread) from six
  browsers: Firefox (all profiles), Chromium, Chrome, Brave, Microsoft Edge and
  Vivaldi across native, Snap and Flatpak paths (`Default` and `Profile N`
  folders).

## Reading mode

On the Mac this is a second view over the article. Here it is done inside the
page in an isolated WebKit script world (`"search-reader"`, `src/reader.rs`):
`assets/reader.js` extracts the article with strict allowlist DOM sanitization
and direct DOM node adoption (no main-world globals and no `innerHTML` re-parse),
sets it in the browser's own type, and keeps the original body nodes in closure
state so exiting restores them cleanly.
That costs no second process and no second load, and Back does not have to be
rewritten to undo it.

The difference a reader will notice: a page that builds itself with JavaScript
after the article is extracted can behave differently under reader mode here,
and scripts that the page relies on to keep running do not keep running.

## Things this build has that the Mac build does not

- A bench socket (`$XDG_RUNTIME_DIR/search-bench.sock`) and `tools/bench.py`,
  so the browser can be driven and measured while it runs. The Mac build has a
  bench tool as well; this one is not a port of it, and its commands are its own.
- `tools/stress.py`, which measures memory and timing per scenario and fails a
  run that misses one of the documented thresholds. The last run that passed
  every check is the one `docs/LIMITS.md` quotes; the run itself prints how many
  checks it made.
- `tools/extensions.py`, which builds three extensions and asserts what the
  loader says about each one, and `tools/check_shield.py`, which compiles the
  generated filter lists with the engine's own parser. The Mac build has no
  equivalent, because neither problem exists there.
- The filter lists here are built from EasyList and EasyPrivacy by
  `tools/build_shield.py` and checked into the repository as JSON, so a rule
  this engine would reject fails a test instead of failing silently.
- `--shot`, `--size`, `--appearance`, `--fresh` and `--bench` flags, for taking
  pictures of the browser and for reproducing a failure on a clean profile.
