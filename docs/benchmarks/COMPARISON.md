# Benchmark comparison: Search vs Chrome, Firefox and Zen

This matrix compares Search (F36) against fresh captures of Chrome, Firefox, and
Zen on the same local fixture page at `127.0.0.1:8099`. The Chrome for Testing
build carries two visible automation banners; those are excluded from design
judgments and noted here so they are not mistaken for product chrome.

## Evidence

- Chrome: `chrome-office.png`
- Firefox: `firefox-office.png`
- Zen: `zen-welcome.png` (first-run state; the real browser is visible)
- Search baseline: `search-office.png`
- Four-up contact sheet: `leading-browsers-contact.png`
- Current F36 captures: `build/council/design-f36-{light,dark}` (36 states each at `1500×918`), `/home/alex/Search Linux/baseline/design-f36-narrow-{light,dark}` (36 states each at `900×1001`), `/home/alex/Search Linux/baseline/motion-evidence-f36/report.json`

All benchmark captures are evidence only; no proprietary asset, logo, or
trademark from Chrome, Firefox, or Zen is copied into Search.

## Matrix (F36 status)

| Facet | Chrome | Firefox | Zen | Search (F36) — comparison |
| --- | --- | --- | --- | --- |
| Visual identity | Neutral gray, dense, conventional | Compact, native, neutral | Calm green, rounded, oversized welcome | **Exceeds** on cohesive warm paper/charcoal + terracotta editorial identity across light and dark without generic card-kit styling. |
| Browser shell | Horizontal tab strip + permanent toolbar | Horizontal tabs + classic toolbar | Minimal vertical shell | **Intentionally differs** (quiet vertical rail, revealable `Ctrl Shift S` toolbar, persistent window controls + footer fold button); matches clarity while preserving calm reading space. |
| Sidebar | No persistent vertical tab list | Optional vertical tabs | Workspace sidebar | **Matches / exceeds** tab scannability with section labels, counts, in-flow New tab row, footer utility icons + fold toggle, and `18px` folded rail. |
| Pinned tabs | Small favicon-only strip | Favicons only | Workspace pins | **Matches** glanceability with a 4-column pin grid and clear active border + terracotta bottom accent indicator. |
| Omnibox | Centered toolbar bar | Toolbar bar + search icon | Floating/minimal bar | **Matches / exceeds** command-palette clarity: site chip, end-positioned caret, grouped Open/History/Search suggestions, editorial left-bar highlight, and keyboard footer. |
| Start page | Branded search + shortcut grid | Portal feed | Oversized welcome hero | **Exceeds** editorial restraint: deliberate typography, focused search field, bookmarks, recent history, and Shield summary; reflows cleanly to one column at `900px`. |
| Reader typography | Browser-dependent | Reader view | Firefox reader view | **Matches / exceeds** calm measure (`660px`), hierarchy, unified `46px` top bar aligned to article measure, code/blockquote styling, and measured WCAG contrast in both themes. |
| Reader size | Page zoom | Reader font menu | Reader font menu | **Matches** with explicit `14px`/`18px`/`22px` readout, `A−`/`A+` controls, and disabled boundary styling verified across all three sizes. |
| Find bar | Native top-right bar | Native bottom bar | Native bar | **Matches** with inline match count, bold danger `.find-zero` zero-match feedback, and clip-free placement below the Reader header at wide and narrow widths. |
| Overlay panels | Separate menus/tabs | Separate popups/windows | Workspace panels | **Matches / exceeds** coherence: six unified rail/search/list panels with compact icon-rail reflow at narrow widths and single scrollbars. |
| Settings | Deep native settings tab | Native preferences tab | Minimal settings | **Intentionally differs** (focused searchable overlay): pinned search field, preserved section headers during filtering, theme previews, single scroller (`38,643px` verified scroll diff), no orphans. |
| Library | Separate managers | Separate Library window | Workspace views | **Matches** unified Bookmarks, History, and Downloads panels with consistent empty and filled states and clear actions. |
| Extensions/shortcuts | Chrome Web Store + manager | Add-ons manager | Firefox add-ons | **Intentionally differs** within documented engine scope: honest compatibility notes, clean `Disable`/`Remove` controls without navigation arrows, and complete keyboard shortcut sheet. |
| Error pages | Minimal error illustration | Native error page | Native error page | **Matches / exceeds** clarity: fault-specific iconography (`ShieldAlert` for TLS, `Refused` for refused, `Offline`, `Clock`, `Search`), safe-first `Go back` CTA on TLS, legible dark secondary buttons. |
| Site info | Toolbar popup | Toolbar popup | Toolbar popup | **Matches / exceeds** with framed semantic security status banner, live per-site Shield pause/resume toggle, zoom controls, permissions, add-ons, and site data clearing, plus framed Peek sheet with primary `Keep`. |
| Dark mode | Neutral dark chrome | Native dark chrome | Green-tinted dark | **Exceeds** warmth and coherence: warm charcoal surfaces (`#1A1816`/`#211F1C`/`#272522`), verified contrast across every token, dark-page opt-in verified in wide and narrow captures, visible scrollbars. |
| Responsive | Strong reflow | Strong reflow | Desktop-oriented | **Matches**: all 36 states captured and verified at `1500×918` and `900×1001` in both light and dark with zero clipping, overlap, double scrollbars, or orphaned headings. |
| Motion | Compositor-accelerated | Native compositor | Animated transitions | **Matches** bounded VSync frame-clock engine with live 60fps multi-frame proof (`fold` 8 frames / `133.3ms`, `omni` 4 frames / `150.0ms`, `panel` 3 entrance frames) and instant single-step reduced-motion (`gtk-enable-animations=false`); software-composited Xvfb scrim fades remain CPU-bound relative to GPU desktop compositors. |

## Intentionally different

Search does not clone Chrome's horizontal tab strip, Firefox's native chrome, or
Zen's green first-run hero. Its vertical rail, warm paper/charcoal palette,
revealable toolbar, editorial start page, unified in-page reader bar, and
six-panel library/settings system are deliberate differentiators.
