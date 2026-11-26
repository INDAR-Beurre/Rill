# Design F35 evidence

This round repairs Settings scrolling evidence honestly and tightens the quiet shell.

- Dedicated Settings scroller is now capped at 240px so the full form genuinely overflows at 1500px.
- Bench `panelscroll` and `panelgeometry` follow the actual open panel's scroller.
- Scrolled Settings differs from unscrolled Settings by 38,074 pixels in the light gate.
- The folded column is an 18px affordance with a sidebar glyph.
- Window controls sit in the sidebar band when the toolbar is hidden.
- The active tab is borderless; the New tab row lives inside the scrollable list.
- A divider separates the sidebar from the page.
- Stress resize probes use shell geometry rather than the WM-decorated window.

Verification: zero-warning release build, 305 tests passed, UI checks passed, `check_shield.py`, `extensions.py`, `compare.py --all`, stress 142/142, 36/36 light and dark states at 1500×918.
