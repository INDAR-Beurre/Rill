# Design round F29 — quiet shell

F29 follows strict F28 reviews that found a 73px ghost toolbar allocation, two-line text inside 34px rows, an unanchored site card, a white-on-white inline banner in dark mode, heavy CSS transitions under reduced motion, and no narrow-window evidence.

## What changed
- Hiding the toolbar now hides the widget itself (`set_no_show_all(true)` + `set_visible(on)`), reclaiming its 72px allocation. Ctrl+Shift+S and Settings can reveal it again.
- Compact tabs are genuinely single-line: subtitles are hidden unless roomy tabs are chosen.
- Pin headers and the pin/open divider stay visible; open-tab headings are restored.
- Reduced-motion desktops get zero-duration CSS transitions through `motion::enabled()`.
- Inline light-background containers are repainted charcoal on forced dark pages instead of receiving white text over white.
- The site card gets a stable page-relative anchor when the toolbar is hidden.
- Bench probes now report actual toolbar visibility and no longer register two `motioncfg`/`toolbar` arms.

## Evidence
- `build/council/design-f29-light` and `build/council/design-f29-dark`: 36 states each at 1500×918, all pixel gates pass.
- Verification: zero-warning release build; 305 tests passed, 0 failed, 1 ignored; UI walkthrough passed; shield, extension and compare checks passed; 142/142 stress checks passed.
- Motion evidence remains limited to visible-change frame counts; it does not claim to prove smoothness.

This round is intentionally minimal. It does not claim the 10/10 benchmark goal; that is for the council to judge against Chrome, Firefox, and Zen.
