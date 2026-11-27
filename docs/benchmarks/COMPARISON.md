# Benchmark comparison: Search vs Chrome, Firefox and Zen

This matrix compares Search (verified Design F39 evidence on base `main` `558c206`,
release binary SHA-256 `c2974efd71a0…`; archived F38 and F37 preserved separately)
against captures of Chrome,
Firefox, and Zen on the same local fixture page at `127.0.0.1:8099`. The Chrome
for Testing build carries two visible automation banners; those are excluded
from design judgments and noted here so they are not mistaken for product
chrome.

## Evidence

- Chrome: `chrome-office.png`
- Firefox: `firefox-office.png`
- Zen: `zen-welcome.png` (first-run state; the real browser is visible)
- Search baseline: `search-office.png`
- Four-up contact sheet: `leading-browsers-contact.png`
- Verified F39 captures: `build/council/design-f39-{light,dark}` (40 states each at `1500×918`, rendered `1500×919` X11), `/home/alex/Search Linux/baseline/design-f39-narrow-{light,dark}` (40 states each at `900×1001`, rendered `900×1002` X11), archived F39 stress report `docs/benchmarks/stress-report-f39.json` (151/151 passed; 182.9 MB warm total PSS / 94.5 MB own / 441.9 MB RSS / 3 processes including after Settings; unmet ~100 MB original target kept explicit), hash-stamped 19-case Wayland GPU motion report `/home/alex/Search Linux/baseline/f39-motion-wayland-gpu/report.json` (19/19 passed on Intel Iris Xe `/dev/dri/renderD128` @ 60 Hz) and X11 software report `/home/alex/Search Linux/baseline/f39-motion-x11/report.json` (19/19 passed), and verified AppImage `build/Search-redesign-x86_64.AppImage` (3/3 consecutive `--quick` stress runs passed at 151/151).
- Historical F38 (`docs/benchmarks/stress-report-f38.json`, 150/150) and F37 (`docs/benchmarks/stress-report-f37.json`, 142/142) evidence remain preserved separately.

All benchmark captures are evidence only; no proprietary asset, logo, or
trademark from Chrome, Firefox, or Zen is copied into Search.

## Matrix (Design F39 verified)

| Facet | Chrome | Firefox | Zen | Search (Design F39) — comparison |
| --- | --- | --- | --- | --- |
| Visual identity | Neutral gray, dense, conventional | Compact, native, neutral | Calm green, rounded, oversized welcome | Warm paper/charcoal + terracotta editorial identity across light and dark without generic card-kit styling; all 7 `docs/screenshots/*.png` gallery captures refreshed from final F39 wide X11 captures. |
| Browser shell | Horizontal tab strip + permanent toolbar | Horizontal tabs + classic toolbar | Minimal vertical shell | **Intentionally differs**: quiet vertical rail, default-hidden revealable `Ctrl Shift S` toolbar (`01b-toolbar`), floating folded window controls (`18-folded-column`) verified live without `SEARCH_BENCH` across toolbar-hidden, toolbar-shown (`top=80px` clearance), and fullscreen sync. |
| Sidebar | No persistent vertical tab list | Optional vertical tabs | Workspace sidebar | Vertical tab list with section labels, counts, in-flow New tab row, footer utility icons + fold toggle, and `18px`/`19px` folded rail. |
| Pinned tabs | Small favicon-only strip | Favicons only | Workspace pins | 4-column pin grid with clear active border and terracotta bottom accent indicator (`02-pins`, `02b-pins-live`). |
| Omnibox | Centered toolbar bar | Toolbar bar + search icon | Floating/minimal bar | Command-palette overlay with honest site security chip (`verified` lock only on live valid HTTPS; `insecure` shield on HTTP and rejected/unknown TLS `04d-omni-tls-failed`; neutral destination chip for unvisited hosts; no search-engine lock on queries `04c-omni-ranking`), grouped Open/History/Search suggestions, local ranking, and zero phantom network typing. |
| Start page | Branded search + shortcut grid | Portal feed | Oversized welcome hero | Native GTK start page with clean sans-serif editorial typography, focused search field, bookmarks, recent history, labeled curated empty profile, and Shield summary; reflows to one column at `900px`. |
| Reader typography | Browser-dependent | Reader view | Firefox reader view | Calm measure (`660px`), hierarchy, unified top bar aligned to article measure, code/blockquote styling, measured WCAG contrast in both themes, isolated `"search-reader"` WebKit script world with strict allowlist DOM sanitization and direct DOM node adoption (verified against adversarial DOM & main-world tampering). |
| Reader size | Page zoom | Reader font menu | Reader font menu | Explicit `14px`/`18px`/`22px` readout, `A−`/`A+` controls, and disabled boundary styling verified across all three sizes. |
| Find bar | Native top-right bar | Native bottom bar | Native bar | Inline match count, bold danger `.find-zero` zero-match feedback, and clip-free placement below the Reader header at wide and narrow widths. |
| Overlay panels | Separate menus/tabs | Separate popups/windows | Workspace panels | Six unified rail/search/list panels with compact horizontal icon-rail reflow and bottom underline at narrow widths, `10px` scrollbar clearance, and single scrollbars. |
| Settings | Deep native settings tab | Native preferences tab | Minimal settings | **Intentionally differs** (focused searchable overlay): pinned search field, preserved section headers during filtering, theme previews, single scroller, and verified true bottom-of-Settings reachability after filter clear across all 4 wide/narrow light/dark captures (`05-panel-settings-scrolled`). |
| Library | Separate managers | Separate Library window | Workspace views | Unified Bookmarks, History, and Downloads panels with consistent empty/filled states, download progress bar, danger failed-transfer styling, and clear actions. |
| Extensions/shortcuts | Chrome Web Store + manager | Add-ons manager | Firefox add-ons | **Intentionally differs** within documented WebKitGTK engine scope: content scripts/CSS/options page only, no background service workers or `browser.storage.*`, honest `why_not` notes, Cairo `pill-switch` toggle (disabled OFF for unsupported extensions) + `Remove`, and complete keyboard shortcut sheet. |
| Error pages | Minimal error illustration | Native error page | Native error page | Fault-specific iconography (`ShieldAlert` for TLS, `Refused`, `Offline`, `Clock`, `Search`), exhaustive WebKit TLS error classification (`09-tls`), safe-first `Go back` CTA focused on TLS initially & on tab revisit without stealing open overlay focus (no bypass), legible dark secondary buttons. |
| Site info | Toolbar popup | Toolbar popup | Toolbar popup | Framed security banner (incl. IPv6 `[::1]` hosts), live per-site Shield pause/resume, zoom, permissions, add-ons, site data clearing, close `×`, true `18px` right inset across wide & narrow windows, alpha-animated click-outside dismiss scrim, zero `Gtk-WARNING`s, plus `Shift`+click Peek card. |
| Dark mode | Neutral dark chrome | Native dark chrome | Green-tinted dark | Warm charcoal surfaces (`#1A1816`/`#211F1C`/`#272522`), verified WCAG contrast across chrome tokens, dark-page opt-in verified in wide and narrow captures, passing dark corner pixel gate (`[26,24,22]`). |
| Responsive | Strong reflow | Strong reflow | Desktop-oriented | 40 states captured at `1500×918` (`1500×919` X11) and `900×1001` (`900×1002` X11) in light and dark, including compact true bottom-of-Settings scroll and `18px` Site Card inset. |
| Motion | Compositor-accelerated | Native compositor | Animated transitions | Bounded first-paint-only VSync `gdk::FrameClock` animator with instant reduced-motion/bench path; all **19/19** live non-`SEARCH_BENCH` cases pass on BOTH isolated Wayland GPU (Intel Iris Xe @ 60 Hz, incl. Peek & live Wayland scrim click-outside) and X11 software `Xvfb` (`panel`, `fold`, `omni`, `find`, `sitecard`, `peek` enter/exit/reversal, zero GTK warnings; GTK3 CPU Cairo + `wf-recorder` readback tradeoffs documented honestly in `docs/design-f39-evidence.md`). |

## Intentionally different

Search does not clone Chrome's horizontal tab strip, Firefox's native chrome, or
Zen's green first-run hero. Its vertical rail, warm paper/charcoal palette,
default-hidden revealable toolbar, native sans-serif editorial start page,
unified in-page reader bar, and six-panel library/settings system are
deliberate differentiators. Platform and engine limits (no cloud sync, accounts,
built-in password manager, multi-window mode, DRM/EME, WebExtensions background
workers/storage API, or cross-app PiP) are documented in `docs/LIMITS.md` and
`docs/DIFFERENCES.md`.
