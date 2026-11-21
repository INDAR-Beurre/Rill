# Changelog

## 1.0.2 (2026-11-21)

### Edge-to-edge chrome, VSync motion, real-site compatibility & security hardening

Verified across the current 34-file Rust codebase (`src/*.rs`, 22,972 lines; ~3.8 MB stripped `target/release/search` binary): **269 Rust unit tests passed (1 ignored, 0 failed)** (`cargo test --release`), **117/117 stress checks passed** (`python3 tools/stress.py`), **18/18 visual states (light & dark)** captured with all pixel checks passing (`docs/screenshots/` updated from `council/round-47` & `council/round-47-dark`), **all extension suite checks passed** (`tools/extensions.py`), and **15,775 WebKit content-blocker rules** across **3 lists** (`network.json`, `hide-0.json`, `hide-sites.json`) compiled cleanly with 0 rejected (`tools/check_shield.py`).

Latest measured `build/stress-report.json` figures:
- One idle tab: **218.1 MB total PSS** (94.2 MB anonymous PSS).
- Tab switch: **1.0 ms mean**, **1.7 ms p95**, **6.5 ms worst** over 300 switches.
- Panel toggles: **22 ms mean**, **101 ms worst** over 80 toggles.

**Chrome & motion:**
- Removed the 23px gray top bar (`deskband`) for a cleaner edge-to-edge layout.
- Native VSync frame-clock motion engine (`src/motion.rs`) paired with tactile GTK3 CSS transitions.
- Added a 3px top page-load progress bar, crossfading tab switches, smooth animated `Ctrl+S` sidebar folding, smooth helm reveal, and soft layered elevation shadows on floating cards.

**Real-website rendering & compatibility:**
- Persistent SQLite cookies (`cookies.sqlite`) in normal browsing (never ephemeral).
- White default page canvas in dark mode unless "Darken pages that stay white" is enabled, with proper `prefers-color-scheme` syncing.
- Enabled WebRTC, UTF-8 default charset, site-specific quirks, locale-derived `Accept-Language` and spellcheck, and a modern Linux WebKit User-Agent.
- Added a clipboard permission prompt (`Kind::Clipboard`) and server-suggested download filenames.
- Fixed the EasyList/EasyPrivacy rule compiler (`tools/build_shield.py`) with `||host/path` parsing, strict modifier handling, and domain-scoped cosmetic rules (`if-domain`) so major CDNs (`google.com`, `gstatic.com`, `googleapis.com`, `cloudfront.net`, `jsdelivr.net`, etc.) are never whole-domain blocked.

**Security & UX hardening:**
- Added a `file://` navigation guard, isolated `search-curtain` script world for element hiding, sanitized reader mode HTML, and a `BeforeUnloadConfirm` escape fix.
- Added a real closed-tab reopen stack (`Ctrl+Shift+T`), a live Downloads panel with a Cancel button, and private-tab isolation across zoom, duplicate, view-source, and element hiding (`curtain`).
- Supports bookmark and history import across 6 browsers: Firefox, Chromium, Chrome, Brave, Edge, and Vivaldi.

### Fixed: the browser died at launch when the disk was full

The release build has 0 warnings, and 236 tests pass with 1 ignored. A fresh
browser launched with its error output on `/dev/full`, where every write
fails, opened its test socket after about 1.4 s, answered a ping, opened a
page, answered again 3 s later and quit cleanly with exit code 0. Before the
fix, the same launch died with SIGABRT (exit 134).

- Cause: `/tmp` was full, so the ad-block lists could not be saved
  ("database or disk is full"). The save callback in `src/shield.rs`
  (`saved`) reported the failure with `eprintln!`. Its log file was on the same
  full disk, so the write failed and `eprintln!` panicked. A panic cannot
  unwind out of a C callback, and the release profile uses `panic = "abort"`,
  so the browser died before its window opened and printed nothing.
- The "bench socket at ..." note in `src/main.rs` failed the same way, just
  after the socket opened.
- Every terminal note now goes through a `note!` helper defined in
  `src/main.rs`. It writes the note straight to stderr and ignores whether
  the write worked, so a note it cannot write is dropped and the browser keeps
  running.
  This covers the display check, the `--shot` notes, the stylesheet load, the
  close-timing trace, a failed import and the three ad-block messages. The only
  `eprintln!` left is in a live DuckDuckGo test in `src/suggest.rs`. That test
  is skipped by default and is never part of the browser.
- When the disk is full, the browser now starts without ad-blocking and says
  on the terminal that nothing is blocked this session. Known gap: the window
  does not show this warning yet.
- Test runs left 775 folders of the browser's own test data in `/tmp`, which
  is what filled it (3.8 GB). They were removed. The list is in
  `build/tmp-cleanup-removed.json`.

### Round 3: Peek, SiteCard and folding the tab column

None of the changes below have been tried on a display yet. 236 tests pass,
1 is ignored, the build has 0 warnings, and the stress suite passes 106 of
106 checks.

- Peek (`src/peek.rs`): Shift-click a link to open it in a card over the
  page. Escape closes the card. Keep moves the page that is already open into a
  new tab without loading it again, so anything typed or scrolled in the card
  stays. A peek from a private tab becomes a private tab. While it is still a
  peek, it refuses permission requests, sign-in, page dialogs, new windows and
  schemes the browser does not handle. Once it becomes a tab, it behaves like
  any other tab.
- SiteCard (`src/sitecard.rs`): Ctrl I, or a click on the site icon in the
  address field, shows the connection's security, the permissions saved for
  the site, its zoom, the add-ons running on it, and a button that clears the
  site's data. Clearing covers the site and the domain it belongs to. Sibling
  sites and look-alike domains are not touched. "Cleared" appears only after
  WebKit confirms the clear, and a failure gets its own message. A private tab
  cannot change saved permissions. Ctrl Shift I still opens the Inspector.
- Ctrl S folds the tab column into a 10px strip. A click on the strip, or
  Ctrl S again, opens it. The fold is saved, and it survives a fullscreen
  video.
- Fixed: opening the tab column showed an empty box. GTK3's `show_all` skips
  a widget marked `no_show_all`, so the column's tabs and controls were never
  shown. The failed-page screenshot check caught it (15,875 bytes; now
  31,070).
- One accent colour, #FF5B2E in light mode and #FF7A55 in dark mode, for
  primary buttons, focus rings, the picked suggestion and the live tab.
  Panels are #FFFFFF in light mode and #202020 in dark mode. A duplicate
  scrim style was removed.
- Tabs using the camera or microphone stay awake, even when muted.
- Import labels name the profiles when a browser has more than one, for
  example "Chrome - Home, Work". Known gap: a Firefox profile that is listed
  in `profiles.ini` but was never used is still named.

### Fixes from critic round 2

None of the changes below have been tried on a display yet. Each has unit
tests for the parts that do not need GTK: 213 tests pass, 1 is ignored, and
the build has 0 warnings.

- Keys: Ctrl H opens History. Ctrl Shift P no longer prints; only Ctrl P
  does. Escape closes the address field or the find bar first, and only then
  answers a permission, sign-in or welcome card. Tab inside an open panel moves
  through the panel's own controls. It is taken only when focus is outside the
  panel.
- Cards take focus when they appear. Permission cards focus their first
  button, the sign-in form focuses the user-name field, page dialogs focus
  their default button, and the welcome card focuses Start. Escape denies or
  cancels, and focus goes back to the page afterwards. The blocked pop-up card
  stays up while focus is inside it.
- One focus ring (2px, ink colour) for buttons, rows, panel tabs, switches,
  fields and suggestion rows (`src/css.rs`). The panel's close button is named
  "Close panel" for screen readers.
- A typed or pasted address loads in the current tab. Only an "Open tab" row
  switches to a tab that already has the page, and never between a private
  tab and a normal one (`src/omnibox.rs`).
- The blocked pop-up card does not come back after "Open it anyway" or after
  its 10 seconds are up. A newer blocked pop-up keeps its own card.
- Typo matching counts letters, not bytes. CJK words get no typo allowance,
  and an accented letter counts as one typo. The status line no longer
  misbehaves when it has no room.
- Import finds Firefox installed from Snap and Flatpak, and finds Chromium,
  Chrome, Brave, Edge and Vivaldi in their native, Snap and Flatpak installs.
  It reads every `Default` and `Profile N` folder and skips folders such as
  `Guest Profile`. A second import is refused while one is running. The Snap
  and Flatpak paths were checked only against a fake home folder. Profile
  names in the labels came in round 3 (see above).
- Add-ons tell the truth before install. An add-on with a background page, or
  with content scripts that call `browser.*` or `chrome.*`, is shown as
  partial: it "loads, but its main part does not run here", and the card
  explains why. Only a real call counts, so comments, sentences and addresses
  such as `https://chrome.google.com` do not. An add-on whose code is all in a
  background page is shown as not running.
- Add-on manifests: names written as `__MSG_` are read from `_locales`, a parse
  error now says where the manifest broke, an odd `user_scripts` value no
  longer blocks install, and `manifest_version` must be 2 or 3.
  `exclude_matches` works: excluded sites go to WebKit as a block list. A
  Firefox add-on gets one id from its gecko id alone, and an older install
  under a different id is replaced, not duplicated.
- An add-on download is matched to the tab that started it, not to whichever
  tab is in front, and a file sent as `application/x-xpinstall` is recognised
  even when it is not named `.xpi`. Add-on style sheets get a real hash for
  their id, and `${` in them is escaped.
- Video fullscreen and F11 stay in step. F11 during a fullscreen video ends
  the page's fullscreen and brings the column back. A video that leaves
  fullscreen keeps the window full if F11 was already on. Switching tabs during
  a fullscreen video ends it and restores the column. The window's state comes
  from `window-state-event`, never read while it is still changing.
- Tabs that recently played sound stay awake. A tab is stamped when its sound
  starts and again when it stops, so it gets the full sleep window after the
  music ends, not counted from when it began. Tabs using the camera or
  microphone became exempt in round 3 (see above).
- Security: import copies other browsers' databases into a private (0700)
  temporary folder. View source writes its file to the user's runtime folder
  with mode 0600 and a name nobody can guess. Search suggestions never send a
  word that looks like a host, an account, a path or a labelled value, and a
  mixed letters-and-digits token of 8 or more characters counts as a secret
  (was 12). The size check for add-on archives counts the bytes actually
  written. Add-on content scripts run in their own script world
  (`search-ext`), apart from the page's. Private tabs get the same safe
  download names and the same Downloads panel rows as normal tabs.

### Earlier in this release

- Typos in the address field are forgiven (`src/fuzzy.rs`). Words of 4 to 7
  letters allow one typo, words of 8 or more allow two, and words under 4
  letters allow none. The first letter has to match or be swapped with the
  second. Typo matches rank below exact ones.
- Keyword shortcuts (`src/keyword.rs`). `yt cats` searches YouTube, and the
  defaults are `yt`, `w`, `gh` and `map`. A template has to be http or https
  with exactly one `%s`, after the host. Settings > KEYWORDS adds and removes
  them.
- Ctrl K switches tabs by typing a name. It lists open tabs only, up to 8.
  Ctrl Shift K duplicates the tab, and the copy stays private if the tab was.
  Picking a page that is already open switches to its tab instead of loading a
  second copy. "Paste and Go" is in the address field's right-click menu.
- Search-engine suggestions (`src/suggest.rs`), off by default. Engines are
  DuckDuckGo, Google, Bing, Brave and Startpage, over https only. A question
  goes out after a 120 ms pause and is given up after 0.8 s. An older question
  is cancelled when a newer one starts, at most 6 rows are shown, redirects are
  not followed, and no cookies or cache are kept. Nothing is sent from a
  private tab, the Ctrl K switcher or a keyword shortcut, or for text that looks
  like an address, path, email, key or card number, or that is over 80
  characters. Picking a suggestion always runs a search, so "google.com" as a
  suggestion searches for that text and does not visit the site. Known false
  negative: a single token of 12 or more characters that mixes letters and
  digits is never sent. Needs system libsoup 3 (`soup3` 0.5).
- A welcome card on first run (`src/welcome.rs`), never shown under `--fresh`.
  Escape or Start closes it and it is not shown again. Settings has an "Import
  from another browser" row (`src/import.rs`). It reads bookmarks and history
  from every Firefox profile and from the Default profile only of Chromium,
  Chrome and Brave. Each database is read from a temporary copy that includes
  its `-wal` file, and nothing is written to the other browser's files. Only
  http and https URLs are kept, duplicates are dropped, history is capped at
  4000 entries, and an import only adds. The databases are read on a worker
  thread (`gio::spawn_blocking`), and only the merge, save and redraw run on the
  main loop. Needs system libsqlite3 (`rusqlite` 0.43, no bundled feature).
- Pop-ups: a page that opens a window without a click is blocked. A card over
  the tab offers "Open it anyway" and hides after 10 seconds. The card belongs
  to its tab: it leaves on a tab switch, comes back when that tab is selected
  again, and is cleared when the tab closes. An allowed pop-up is built with
  `related_view`, so `window.opener` works, and a private opener stays private.
  `tools/fixtures.py` serves `/popup-auto` and `/popup-target`. `tools/stress.py`
  checks that no tab appears without a click, and that the spray adds at most
  1500 MB (`spray_growth_mb`). Both checks passed in the last stress run.
- Memory: `page_memory_pressure()` in `src/web.rs` sets a 384 MB limit, with
  conservative at 0.33, strict at 0.5, and a 5 s poll. It is used by
  `shared_context()`, which replaced `WebContext::default()`, by
  `ephemeral_context()`, and by the network process (set in `src/main.rs` after
  `gtk::init`). It did not lower the memory-spray peak: 1171.7 MB in the last run, against
  1141 MB in an earlier run. The likely cause is live JavaScript objects, which has not been
  measured. The 100 MB total PSS target is still missed.
- Tabs go to sleep after 10 minutes without being looked at (was 30). A tab
  playing audio is never put to sleep.
- YouTube plays with sound at 720p (1280x720) and 1080p (1920x1080). dav1d is
  not installed, so AV1 decoding is unverified.
- The `fullscreen` verb in `src/bench.rs` (`on`, `off`, `report`) checks video
  fullscreen. A run hid the tab column and bookmark row on `on` and showed them
  again on `off`, with 0 Gtk-CRITICAL lines. A real video entering fullscreen
  through the page has not been re-run with this check.
- Enter in a page's dialog presses the focused button, so Tab to Cancel and
  Enter cancels. With focus elsewhere, Enter still means OK.
- The user agent names the application "Search" and the package version
  (`src/web.rs`).
- A download from a private tab is not saved to `downloads.json`.
- An error status with a page of its own (a site's 4xx or 5xx HTML) is shown
  as the site sent it. Only an answer with no body gets the browser's own error
  page.
- `tools/report_figures.py` reads a leading minus sign ("-0.1"). A range such as
  "3-5" still reads as 3 and 5.

- Extensions has a "Firefox Add-ons" row. Its Open button opens
  addons.mozilla.org in a new tab. An add-on's `.xpi` from there is downloaded
  and given to the same installer as a file chosen in the panel, so both get
  the same checks. The store serves add-ons as `application/x-xpinstall`, which
  the engine left alone: the tab sat on the `.xpi` address and nothing was
  downloaded. The browser now asks for a download itself. This was checked with
  uBlock Origin 1.76.6: one file in Downloads, one install, listed in
  Extensions. Whether the store's own "Add to Firefox" button offers the file
  in this browser has not been checked.
- A Firefox add-on is known by the id it gives itself
  (`browser_specific_settings.gecko.id`, or the older `applications` key), not
  by the name of the file it came in. Downloading it again, which saves
  "name (1).xpi", or installing a newer version now replaces the installed copy
  instead of adding a second one beside it.
- A page load that the browser turns into a download is no longer treated as a
  failed load. WebKit ends such a load with "Frame load interrupted"
  (WebKitPolicyError 102). The browser took that for an error, marked the tab
  failed, and the same address was asked for again, so every add-on from the
  store downloaded twice. The tab now stays on the page it was on.

- A page's alert, confirm, prompt and leave-page question is answered by a
  card over its tab, with the page's host on it. Escape cancels and Enter
  confirms. A background tab waits until it is selected. A third dialog within
  ten seconds offers "Stop this page asking". Not yet clicked through on a
  display.
- A file upload opens the desktop's own file chooser, with the page's MIME
  types and multiple selection. Cancel refuses the upload. Not yet clicked
  through on a display.
- Hovering a link shows its address in a small line at the bottom left of the
  page, taken from the engine's hit test and never from the page's title. The
  middle of a long address is cut. Not yet clicked through on a display.

- A page that asks for notifications, location, the camera or the microphone
  gets a small card over the page with Allow and Block. The answer is kept per
  site in `permissions.json` and can be cleared in Settings. A private tab asks
  every time and keeps nothing. Device info, media keys, pointer lock, clipboard
  and website data are still refused without asking. A site that asks for a
  user name and password gets a form over its tab, and nothing is saved.
  Not yet clicked through on a display.
- The right-click menu on a link offers "Open link in new tab", "Open link in
  private tab" and "Copy link address". A link in a private tab only opens
  privately. With text selected it offers "Search DuckDuckGo for the selection"
  (the engine from Settings), and on a web page "Keep this page". On a page
  that is already kept, the item reads "Stop keeping this page". It works like
  Ctrl D and removes the bookmark, so the label says what it will do instead of
  offering to keep the page again. WebKit's copy, paste and inspect items stay.
  The menu has not yet been clicked through on a display. WebKit adds each item's
  action to its own `context-menu` action group, which was confirmed from the
  installed library rather than from a live session.
- Zoom is remembered per host for the session, so a site opens at the zoom it
  was last set to. Ctrl 0 resets it. Ctrl P prints the page through the engine's
  own print dialog, and it is listed on the Shortcuts panel.
- A bad certificate now stops the page. This covers a certificate that has
  expired, is not yet valid, names another host or comes from an authority the
  system does not trust. The error page says which of those it was and ends
  with "It was not opened." It offers only "Try again" and never a way through.
  It was checked against expired, self-signed, wrong.host and untrusted-root
  on badssl.com. Revocation is not checked, because WebKitGTK does not check it.
  `docs/LIMITS.md` says so.
- A tab whose page process dies says "The page crashed." and offers to try
  again.
- The AppImage now includes GStreamer's autodetect plugin (`autoaudiosink`)
  and `libgstaudiofx.so`. `tools/appimage.sh` refuses to build without them.
  Without that plugin, the engine's page process aborted on any site that sets
  up audio. GitHub crashed on every load on the machine this was measured on.
  With it, the AppImage stayed up on GitHub for 30 seconds with no page-process
  crash.
- `wait_loaded()` in `tools/bench.py` now waits for the page to load in the
  active tab. Before, it read a `loading` field the probe never sends, so it
  returned while the page was still `about:blank`. Four stress checks failed
  because of it: page text, settling after the memory spray, Shift F3 and Tab
  into the panel. The full run now passes 101 of 101 checks.
- Contrast raised across secondary text and the borders that are not text, so a
  hint and a hairline are legible on both grounds instead of only on the dark
  one. The quiet grey the hints are set in went from 1.48:1 on white to 5.42:1
  (`#6A6A68` in `src/css.rs`), the muted grey from 5.10:1 to 6.48:1, the input
  border from 1.25:1 to 2.08:1 light and 1.37:1 to 3.04:1 dark, and the
  hairline from 1.23:1 to 1.51:1 light and 1.37:1 to 1.54:1 dark. Ratios
  computed with the WCAG formula from the hex values in `src/palette.rs` and
  `src/css.rs`.
- Square window corners. A radius on the frame showed on one edge and was
  clipped on the other, which read as a smudge rather than a shape.
- A pin with no favicon shows its letter, and pins wrap to the width of the
  column instead of the sixth one being cut off at the edge.
- The address field no longer repeats the host above its own text, and no longer
  slides while you type: its position comes from the window height alone, so a
  suggestion arriving in the list below cannot move it. It has a visible focus
  edge of its own.
- Settings: switches draw a track and a knob of the right size, and the chosen
  appearance chip is visibly chosen.
- One inset for the panel card, so every panel starts at the same place.
- Return in Find steps through the matches; Shift Return steps back.
- `F11` full screen, `Ctrl U` view source, and `Escape` stops a page that is
  still loading.
- An error page can be left with the mouse, and a 4xx offers to change the
  address instead of reloading one that is not there.
- The shield loads its compiled lists from the engine's own store after the
  first launch, rather than recompiling 14,000 rules every time.
- The suite asserts the numbers this documentation quotes, so a threshold that
  drifts and a page that drifts cannot disagree without a red run.

## 1.0.1

- AppImage now loads WebKit's own packaged process helpers and fonts. The
  original package silently used `/usr/lib` helpers and left a real page stuck
  loading; AppRun's MIT-licensed relocation hooks fix that boundary.
- Error pages paint the browser's readable reason, address and action instead
  of being hidden by the WebView snapshot beneath them.
- Downloads can be searched, with progress refreshes keeping the search.
- Extensions can be installed through a native file/folder chooser.
- Ctrl Left and Ctrl Right switch panels; Tab moves focus into the open panel.
- The packaged candidate passed 123 Rust tests and 101 browser checks.

## 1.0.0

The first Linux build.

- A column of tabs with pinned pages, a live row, and a field over the page.
- WebKitGTK as the engine: no Chromium inside, about 2.8 MB of binary, stripped.
- Ad and tracker blocking as a compiled content blocker, before the request,
  built from EasyList and EasyPrivacy (the two lists uBlock Origin ships
  enabled) by `tools/build_shield.py`. None of that extension's code runs here.
- Hidden elements per site, applied before the first frame.
- Reading mode, find, downloads, bookmarks, history, settings, and a Shortcuts
  panel so the keys are discoverable.
- Light, dark, or the desktop's own.
- Extensions: content scripts, their CSS, host patterns and options pages, from
  a `.crx`, an `.xpi` or a folder, with the panel stating what will not run.
  There is no storage API, so an extension that keeps state in
  `browser.storage.local` keeps none. Read the `## Extensions` section of the
  README before expecting a Chrome extension to just work: WebKitGTK has no
  WebExtensions API, so an extension whose work lives in a background page is
  installed and marked as not running rather than quietly doing nothing.
- A bookmarks bar, hidden until there is something in it.
- A resizable window whose size and maximised state are remembered.
- *Use less memory* in Settings: one idle tab measured 62 to 66 MB of the
  browser's own memory with it on, across four runs, against 60 to 88 MB with
  the two always-on knobs and 87 to 104 MB with nothing set. The switch applies
  at the next launch.
- Two engine knobs that are now always set, because four runs each showed they
  cost nothing to keep: no DMA-BUF renderer, and two allocator arenas instead
  of one per thread. Turning compositing off on its own was measured and made
  memory worse, so that one stayed a switch.
- A bench socket and a stress suite, so the browser can be tested while it runs.

### Fixed while it was being scored

Each of these was found by a critic or by the suite, not by guessing:

- A pop-up took the screen while the tab bar still named the tab it opened
  from, so `Ctrl W` closed the page the reader was looking at.
- An error page belonged to the window rather than to its tab: a 404 in a
  background tab painted over the page being read.
- Reading mode was inert for two rounds: its script interpolated a palette
  containing a quote into JS source, so the whole file failed to parse.
  `node --check` guards it now, and the mode carries its own way out.
- A panel cut its last row in half, and sat 115px left of centre.
- Find printed 4294967295 matches, because `found-text` reports the text it
  found and `-1` until it has counted. It reads `counted-matches` now.
- Reopening a closed tab deleted the history entry it was reopened from.
- Every `Shift` shortcut was dead, because GDK reports the upper-case letter
  when Shift is held and the key was matched by its lower-case name. The key
  table is now a pure function with its own tests, and the suite drives the
  same one the keyboard does.
- The blocker did nothing on 9,565 of its 14,000 rules and then nothing at all:
  WebKit rejects any filter containing an alternation, and one bad rule fails
  the whole list. `tools/check_shield.py` now compiles the generated lists with
  the engine's own parser.
- `window.open` from a private tab opened a public tab, and a private page's
  address went into the history file.
- An extension's CSS was accepted by WebKit's user style sheet and had no
  effect at either level; it is injected at document start now, which works.
