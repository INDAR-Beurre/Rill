# Benchmark comparison: Search vs Chrome, Firefox and Zen

This matrix is the baseline for the benchmark-relative design review. It compares
fresh captures of the same local fixture page at 127.0.0.1:8099. The browser
Chrome for Testing build carries two visible automation banners; those are
excluded from design judgments and noted here so they are not mistaken for
product chrome.

## Evidence

- Chrome: `chrome-office.png`
- Firefox: `firefox-office.png`
- Zen: `zen-welcome.png` (first-run state; the real browser is visible)
- Search: `search-office.png`
- Four-up contact sheet: `leading-browsers-contact.png`

All files are in this directory. The benchmark captures are evidence only; no
proprietary asset, logo or trademark from Chrome, Firefox or Zen is copied into
the product.

## Matrix

| Facet | Chrome | Firefox | Zen | Search — baseline conclusion |
| --- | --- | --- | --- | --- |
| Visual identity | Neutral gray, dense, highly conventional | Compact, native, neutral | Calm green, rounded, oversized welcome | Warm paper/terracotta editorial language; **exceeds** the benchmark set on identity distinctiveness. |
| Browser shell | Thin horizontal tabs, separate toolbar | Native vertical space, classic controls | Minimal first-run shell | Vertical tab rail plus one 72px toolbar; **matches** clarity while preserving a distinct information architecture. |
| Sidebar | No tab list in normal Chrome | No persistent tab list | Workspace-oriented sidebar | Persistent named tab list and section labels; **exceeds** tab discoverability. |
| Pinned tabs | Small favicon-only strip | Favicons only | Workspace grouping | Labelled pinned column; needs stronger active contrast. |
| Omnibox | Single centered field | Single field with separate search icon | Hidden during first-run | Centered field with security chip, grouped suggestions and keyboard hints; must render every row in captures. |
| Start page | Google/new-tab grid | Mozilla portal | Zen welcome hero | Editorial "Go somewhere interesting" composition with bookmarks/history/shield; **exceeds** editorial quality but must stay clip-free narrow. |
| Reader typography | Browser-dependent | Reader mode available | Not shown | Purpose-built calm reader with structured headings and links; strong baseline. |
| Reader size | Browser zoom | Reader controls | Not shown | Three explicit 14/18/22px steps; needs all three captured. |
| Find bar | High-contrast native bar | Native bar | Not shown | In-product bar with count; needs zero-match and narrow collision evidence. |
| Overlay panels | Menus/popups, no unified library | Separate panels | Workspace panels | Six consistent rail/search/row panels; needs filled Extensions and filtered Settings captures. |
| Settings | Deep native settings tree | Native preferences | Minimal settings | Searchable in-browser Settings with theme previews; needs filtered-state evidence. |
| Library | Separate history/bookmarks UI | Separate native managers | Workspace views | Unified Bookmarks/History/Downloads panels; needs empty/filled captures for all. |
| Extensions/shortcuts | Web Store + chrome://extensions | Add-ons manager | Minimal extension story | crx/xpi/unpacked plus shortcut reference; needs installed-extension capture. |
| Error pages | Minimal standard error page | Native error page | Not shown | Editorial error pages with a reason and recovery action; needs TLS hierarchy evidence. |
| Site info | Browser popup | Browser popup | Not shown | Purpose-built site card and Peek; needs HTTPS and narrow-inset captures. |
| Dark mode | Strong generic dark theme | Native dark theme | Green-dark welcome | Warm charcoal system; needs contrast evidence for scrollbars and keycaps. |
| Responsive | Strong reflow | Strong reflow | Limited first-run evidence | Compact reflow exists; needs narrow dark captures and no overlap/orphan headings. |
| Motion | Polished compositor effects | Native effects | Strong animated welcome | Bounded VSync motion engine; needs representative transition and reduced-motion evidence. |

## Intentionally different

Search does not clone Chrome's tab strip, Firefox's native chrome, or Zen's
first-run green hero. Its vertical rail, editorial start page, warm surfaces and
in-product reading/library surfaces are deliberate differentiators. The review
criterion is whether each state is as legible, stable and complete as the
benchmarks, not whether it looks identical.

## Known gaps to close

1. Omnibox suggestion rows must be visible in captured evidence.
2. Capture all explicit reader-size, extension, find-count, TLS, HTTPS site-card
   and filtered-settings states in light, dark and narrow layouts.
3. Provide motion frame/timing and reduced-motion evidence.
4. Resolve any benchmark-relative contrast or overlap regressions before the
   three-seat council round.
