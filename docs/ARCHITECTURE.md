# How Rill is put together

Rill (internal Cargo package/binary `search`) is a small surface with a lot underneath. This is the module map for
the current local build:

```
main.rs           flags, the environment, the window, and the loop
app.rs            the browser: tabs, active space, session restoration, and every action
spaces.rs         persistent named tab groups, Default-space safety, and name/ID validation
web.rs            WebKitGTK 4.1 setup, persistent SQLite cookies vs ephemeral private sessions, locale Accept-Language & spellcheck, WebRTC, downloads, error pages
ui.rs             GTK3 shell: top tab strip, optional Ctrl S vertical tab sidebar, customizable mouse-first navigation toolbar (`Ctrl Shift S`), live rounded viewport corner masks, and 3px load progress
startpage.rs      native start page with cached 8x8 Bayer ordered-dither horizon, direct search/autocomplete, customizable bookmark shortcuts, and private-session isolation
decor.rs          Linux window decoration negotiation (`System`, `Browser`, `Hidden`) honoring EWMH WMs and `gtk-decoration-layout`
addons_catalog.rs bounded user-initiated HTTPS search and `.xpi` staging against official `addons.mozilla.org` with strict redirect/size guards and honest capability badges
motion.rs         VSync `add_tick_callback` frame-clock motion engine for fades, slides, pops, toasts, and paned sidebar fold (disabled under `SEARCH_BENCH` or `gtk-enable-animations=false`)
css.rs            one stylesheet, built from the palette
palette.rs        every colour, as a light/dark pair
omnibox.rs        the field over the page
find.rs           the find bar
panels.rs         bookmarks, history, downloads, settings, shortcuts, extensions
peek.rs           Shift+click floating preview card with its own WebView, Keep/Close controls, and compositor-independent snapshot capture
sitecard.rs       site identity, TLS status, per-host zoom, saved permissions, active add-ons, and site-data clearing card
reader.rs         reading mode (assets/reader.js in isolated "search-reader" WebKit script world with strict allowlist DOM sanitization)
curtain.rs        hidden elements, per site, before first paint, with element picking in the isolated `"search-curtain"` WebKit script world
shield.rs         the content blocker, compiled once from the generated lists
extensions.rs     content scripts and CSS, limited to the sites they ask for
keys.rs           every shortcut: what a key means, and what it does
fuzzy.rs          typo-tolerant matching for the address field
keyword.rs        keyword shortcuts (`yt cats`), and their templates
menu.rs           the right-click menu: edits WebKit's own, keeps what it has
permissions.rs    site permission & HTTP auth prompts (camera, mic, geolocation, notifications, clipboard, add-on install), `permissions.json`
dialogs.rs        alert, confirm, prompt and file-chooser cards over a tab
status.rs         the link address line at the bottom left of the page
suggest.rs        search-engine suggestions (Startpage default, opt-out in Settings)
import.rs         bookmarks and history from Firefox, Chromium, Chrome, Brave
welcome.rs        full-tab first-run introduction with live draft previews and explicit optional starter sites
js.rs             running a script in a page, and hearing back
store.rs          prefs and the small JSON files
files.rs          reading and writing those files
bench.rs          the socket tests drive the browser through
layer.rs          where the things over the page stand
icons.rs          the marks the UI draws for itself
gtkx.rs           signal returns
shield_assets.rs  the generated filter lists, included at build time
```

`src/shield_assets.rs` is generated, not written: `tools/build_shield.py` turns
EasyList and EasyPrivacy into the content blocker's JSON and emits a `FILTERS`
constant of `include_str!`. Regenerate it, do not edit it. `assets/shield/` now
ships 3 lists:
- `network.json`: the network list at the tool's cap of 14,000 rules
- `hide-0.json`: global cosmetic rules (one hide-element rule carrying 2,626
  selectors: the 26 hand-written cookie-bar and interstitial selectors plus
  2,600 global selectors taken from the lists)
- `hide-sites.json`: 1,774 domain-scoped cosmetic rules carrying `if-domain`
  triggers so site-specific hiding rules cannot hide unrelated elements on
  other domains

`shield.rs` compiles each list once and hands the result to the engine's own
filter store, keyed by the hash of the list's bytes. A later launch loads the
compiled filter back instead of recompiling 14,000 rules: the key changes when
the list changes, so a new EasyList pull cannot be answered with last month's
rules.

`src/curtain.rs` hides per-site user-picked elements via host-scoped
`UserStyleSheet` entries (`allow_list` per host, with separate in-memory
overrides for private tabs that never mutate `hidden.json`) before first paint,
and runs element picking in the isolated `"search-curtain"` WebKit script world
(`CURTAIN_WORLD`), keeping `searchHide` message handling separate from page
scripts so untrusted page JS cannot invoke `searchHide` or tamper with picker
state.

## One tab

`app::Tab` holds the identity: id, space_id, url, title, pinned, private, and the widgets
for its row and pin. The `WebView` is not there. It is made by `web::view_for`
the first time the tab is selected, added to a `GtkStack` under the name
`tab-<id>`, and removed when the tab closes. A tab restored from the session
file therefore costs no web process until it is looked at, which is most of why
opening with twenty tabs is still instant.

`web.rs` handles WebKitGTK 4.1 setup, persistent SQLite cookies for normal
sessions vs ephemeral private sessions, locale `Accept-Language` and spellcheck
configuration, WebRTC settings, download tracking and cancellation, and error
pages. Each view reports itself through signals that `web::attach` wires: title,
uri, load state, navigation state, a page asking for a window, a page asking to
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

`ui::build` supports two exclusive tab layouts: a horizontal top strip or a
vertical sidebar (`Ctrl S`). `app::sync_tab_layout` reapplies the visibility
plan after broad GTK visibility updates, so showing one tab list cannot revive
the other. Both layouts have compact favicon-only pins with a monogram fallback,
accessible names, and context actions. Browser menus, new-tab actions, and
window controls remain reachable when the navigation toolbar is hidden.

The page column packs a customizable navigation toolbar (visible by default for
new profiles; `Ctrl Shift S` toggles it), the optional bookmarks bar, and a
`GtkOverlay` containing the page `GtkStack` with an overlaid 3px load-progress
bar. Viewport corner masks round live WebKit content as well as native pages.
Fullscreen hides browser chrome and restores its layout when it ends.

`startpage::StartPage` owns a native scrolled start surface under the stack's
`blank` child. Its direct entry resolves URLs/search/keywords through the same
navigation path as the omnibox. Bookmark shortcuts use cached real site icons
and configurable label modes; private mode hides regular browsing records.
The dither canvas reads the live theme, so appearance switches do
not leave stale artwork. No additional WebView or network request is needed
for the start page.

The introduction is a separate native `welcome` child of that same stack,
filling the tab viewport. A scrollable settings body and a fixed action footer
keep Back, Next, Skip, and Finish reachable on short windows. Appearance,
layout, and search changes are drafts until completion; Skip/Escape reverts
unconfirmed choices. Showing setup makes no favicon requests. Selecting an
optional starter site explicitly allows its official icon request.

`app::refresh_helm` updates the persistent address, navigation sensitivity,
reload/stop glyph, bookmark/reader state, protection tooltip, and tab count.
HTTP remains visibly prefixed; embedded credentials are omitted. Video
fullscreen hides both the column and toolbar, restoring their previous state
when it ends.

At compact widths, some navigation and page controls leave the toolbar to keep
the address usable. Their actions remain available through the native browser
overflow menu. The current GTK walkthrough checks control bounds and overflow
behavior rather than relying on the older shell's fixed screenshot coordinates.

Rows are rebuilt when the set of tabs changes, not on every title change: the
browser keeps the row's widgets on the tab and updates the label in place.

The bookmarks bar is a row of chips in the page's column, above the page stack
overlay and below the navigation toolbar. It is hidden when nothing is kept and
shown again when something is, and Settings has a switch for it; both live in
`app::render_bmarks`, which is why a bookmark taken with `Ctrl D` appears
without rebuilding the column.

`motion.rs` drives UI transitions with a VSync `add_tick_callback` frame-clock
motion engine for fades, slides, pops, toasts, and paned sidebar fold; it is
disabled automatically under `SEARCH_BENCH` or `gtk-enable-animations=false`.

Nothing in `ui.rs` knows what a tab is. `app.rs` touches GTK deliberately in
`app::capture` (and the `draw_at`, `opacity_of` and `draw_into`
helpers under it), walking the real widget tree with cairo because a screenshot
of a window with no compositor has to draw the widgets that were actually built,
at the offsets they were actually given. Grep `fn capture` to see the shape of it.

## Spaces and session ownership

New profiles contain only **Default**. Settings manages additional named
spaces. Spaces group tabs within the same browser profile: cookies, logins,
bookmarks, and history remain shared. They are not isolated containers.

`spaces.json` holds stable IDs, names, and the selected space. Session tab
records carry their space ID and per-space active markers. Older records with
no space ID migrate to Default; unknown IDs resolve there as well. Switching
spaces changes the visible tab set and restores its last active tab. Private
tabs stay in memory and are excluded from session serialization. Deleting a
custom space moves its tabs into Default; Default itself cannot be deleted.

## Things over the page

`layer::Layer` is a `GtkFixed` that knows how to place a child for a window of a
given size. The omnibox, the find bar, and the panels each own one, are added to
the root overlay, and are laid out when the window changes size. Alongside them
are the floating cards and prompts:
- `peek.rs`: Shift+click floating preview card with its own `WebView`, Keep/Close
  controls, and compositor-independent snapshot capture.
- `sitecard.rs`: Site identity, TLS status, per-host zoom, saved permissions,
  active add-ons, and site-data clearing card.
- `permissions.rs`: Site permission & HTTP auth prompts including camera, mic,
  geolocation, notifications, clipboard, and add-on install.

This overlay structure is why the scrim covers the window and the field lands in
the middle, and why the same layout is reproducible in a screenshot without a
compositor.

## Looking without a compositor

`app::capture` draws the frame into a cairo surface, asks WebKit for a snapshot
of the visible page (and any visible `peek.rs` preview WebView), lays that into
the page's rectangle, and then draws the over-page layers on top. This captures
the actual GTK widget tree and WebKit page, not a simulated browser interface.

`tools/test_ui.py` is the GTK widget/behavior walkthrough (isolated profile,
real GTK entries and buttons, light/dark switching, wide/medium/compact
reflow, private-tab isolation, and stderr cleanliness). `tools/shots.sh` is
the 40-state light/dark/narrow screenshot and pixel-gate capture suite
rebaselined to the warm paper/charcoal palette (`1500×918` wide and `900×1001`
narrow), including true fresh-profile start (`08a-start-fresh`), failed-TLS
omnibox (`04d-omni-tls-failed`), true bottom-of-Settings geometry verification,
and Site Card `18px` inset + dismiss-scrim verification. Repository gallery
images in `docs/screenshots/` are refreshed directly from the final Design F39
wide captures (`build/council/design-f39-{light,dark}`). See [TESTING.md](TESTING.md).

## Memory

One idle tab consists of three core processes — the GTK frame, a
WebKit web process, and a WebKit network process. In Design F38, replacing the
named GTK filter icon (`edit-find-symbolic`) with Cairo `Icon::Search` removed
F37's `glycin-svg` + 2 `bwrap` regression at idle and after History/Bookmarks;
in Design F39, replacing Settings' `GtkSwitch` and `GtkComboBoxText` with native
Cairo `pill-switch` (`GtkToggleButton`) and `engine-picker` (`GtkMenuButton` +
`GtkPopover`) eliminated the remaining `glycin-image-rs` + 2 `bwrap` spawn as
well, holding the process tree at **3 processes** across idle, History,
Bookmarks, Settings open, and after Settings closed (both source and AppImage).

`tools/stress.py` measures total PSS (`pss_mb`), anonymous own PSS (`anon_mb`),
shared memory (`shmem_mb`), `private_dirty_mb`, RSS (`rss_mb`), and process
count for the whole process group. The archived Design F39 full run
(`docs/benchmarks/stress-report-f39.json`, 151/151 passed) recorded one idle
tab at **182.9 MB warm total PSS** (185.3 MB cold), **94.5 MB own anonymous
PSS**, and **441.9 MB RSS** across 3 processes (archived F38: 146.1 MB PSS / 3
idle procs, 6 after Settings; archived F37: 188.1 MB PSS / 6 procs;
historical `1.0.2` baseline: 218.1 MB total PSS, 94.2 MB anonymous PSS, 318.2 MB RSS).
The original ~100 MB total-PSS request remains **unmet**; `tools/stress.py`
enforces a rebaselined 230 MB WebKitGTK floor. Anonymous PSS is not total RAM:
shared libraries and other file-backed mappings also contribute to total PSS.

Before GTK starts, `src/main.rs` always sets `MALLOC_ARENA_MAX=2` and disables
the Wayland DMA-BUF renderer by default with `WEBKIT_DISABLE_DMABUF_RENDERER=1`
(opt-in via `SEARCH_ENABLE_DMABUF=1` or `WEBKIT_DISABLE_DMABUF_RENDERER=0`) while
keeping GPU compositing enabled unless `low_memory` is active. Earlier baseline
runs measuring `WEBKIT_DISABLE_DMABUF_RENDERER=1` and `MALLOC_ARENA_MAX=2` took one
idle tab from 87-104 MB of the browser's own memory down to 60-88 MB and moved
resident memory from 212.8 MB to 159.0 MB. `SEARCH_LOW_MEMORY=1` (or Settings →
*Use less memory*) forces DMA-BUF off and adds `WEBKIT_DISABLE_COMPOSITING_MODE=1`,
which brought the same four runs to 62-66 MB and is left opt-in because
disabling compositing changes how pages animate; both are read before the engine
starts, so preference changes apply on the next launch. Tabs also let go of
their page after ten minutes without being looked at (Settings → *Let tabs
sleep*), which returns the page's memory to the system.
A tab that is playing sound is never put to sleep. The network process gets a
384 MB memory limit (`WebsiteDataManager::set_memory_pressure_settings` in
`src/main.rs`, set before the first context exists): it frees caches at a third
of that and everything it can at half, checking every 5 seconds. Page processes
get the same limit from `web::page_memory_pressure()`, through the contexts
`shared_context()` and `ephemeral_context()` build.

See [TESTING.md](TESTING.md) for the historical `1.0.2` timing and memory baseline
and [LIMITS.md](LIMITS.md) for memory definitions and limitations.
`python3 tools/report_figures.py` prints whichever figures are currently saved
in `build/stress-report.json`; reading that report does not rerun the suite or
measure the redesign.
