# Design F36 evidence

Commit: `main` (F36 — dark warmth, responsive reflow, motion cadence, and surface polish).

F36 closes the concrete visual, responsive, Settings, Reader, Site Card, Error, and live-motion defects surfaced across F31–F35 reviews.

## Changes

- **Live VSync motion cadence (`src/motion.rs`)**: Anchored `start_us` on the first frame-clock tick (`now - 16_667`) instead of reading a stale idle `frame_clock().frame_time()` before synchronous widget construction. Live 60fps Xvfb captures without `SEARCH_BENCH` now show multi-frame transitions when enabled (`fold`: 8 frames / 133.3ms vs reduced 2 frames / 16.7ms; `omni`: 4 frames / 150.0ms vs reduced 1 frame / 0ms; `panel`: 3 entrance frames vs reduced 1 instant frame).
- **Warm dark charcoal palette & scrollbars (`src/palette.rs`, `src/css.rs`)**: Shifted dark surfaces from cold grey to warm editorial charcoal (`#1A1816` ground, `#211F1C` card, `#272522` surface, `#EFECE6` ink, `#362A24` warm tint) while keeping every WCAG contrast test and dark-corner gate green (`[26, 24, 22]`). Raised scrollbar slider contrast and secondary `.act` button surface contrast in dark mode.
- **Settings filter, scroller, and nested-scrollbar fix (`src/panels.rs`)**:
  - `filter_settings_list` now preserves `panel-section-header` boxes (fixing the missing `+ Appearance` heading on filtered queries) and allows section-title matches without leaking orphaned subtitles.
  - Disabled the outer `ScrolledWindow` vertical policy while Settings is open, eliminating the double scrollbar thumb beside the search field at narrow widths.
  - Raised `settings_scroll` from `240px` to `260–380px` (`vexpand(true)`), keeping genuine overflow (`38,643` changed pixels light, `38,013` dark; `35,669` narrow light, `34,976` narrow dark).
- **Extensions toggle clarity (`src/panels.rs`)**: Removed the misleading navigation `ArrowRight` icon from `Disable` / `Enable` toggle buttons.
- **Unified Reader header & narrow Find clearance (`assets/reader.js`, `src/find.rs`, `src/css.rs`)**:
  - Replaced detached top-corner Reader buttons with a fixed `46px` unified header aligned to the `660px` article measure (`A−`, active size readout, `A+` with explicit `:disabled` styling, and `Leave reading mode`).
  - Removed the duplicate vertical offset in narrow reading Find (`src/find.rs`), so narrow Find sits cleanly below the unified Reader header.
  - Added `.find-zero` danger styling for `"no matches"` and moved bench `omnitext`/`find` carets to end-of-text.
- **Error page iconography (`src/icons.rs`, `src/app.rs`)**: Added `Icon::ShieldAlert` (shield with `!`, replacing checkmarked `Icon::Shield` on TLS failures) and `Icon::Refused` (circle with slash, replacing hollow `Icon::Stop` on connection refused).
- **Site Card & Peek (`src/sitecard.rs`, `src/peek.rs`, `src/css.rs`)**: Framed Site Card security status in a semantic icon banner (`Lock` / `ShieldAlert`), added a live per-site Shield pause/resume toggle (`app.toggle_blocker()`), and added a hairline header divider + primary `Keep` CTA to Peek.
- **Sidebar & pinned tabs (`src/ui.rs`, `src/app.rs`, `src/css.rs`)**: Added a quiet footer fold toggle button (`side_fold`) when the toolbar is hidden, centered the `18px` folded rail icon cleanly, added a crisp accent underline to `.pin.live`, and softened `.suggest .sgrow.picked` to an editorial left accent bar.

## Verification

- `cargo build --release`: zero warnings.
- `cargo test --release`: 305 passed, 0 failed, 1 ignored.
- `tools/test_ui.py` (`SEARCH_TEST_BACKEND=x11`): passed (zero GTK criticals, zero invalid allocations).
- `tools/stress.py`: 142/142 passed (`183.9MB` resident, `93.3MB` own for one idle tab).
- `tools/check_shield.py`: 15,775 rules accepted, 0 rejected.
- `tools/extensions.py`: all extension checks passed.
- `tools/compare.py --all build/council/design-f36-light`: passed at `1500×918`.
- Wide captures (`1500×918`): `build/council/design-f36-light` (36/36 + gates), `build/council/design-f36-dark` (36/36 + gates).
- Narrow captures (`900×1001` via `NARROW=1`): `/home/alex/Search Linux/baseline/design-f36-narrow-light` (36/36 + gates), `/home/alex/Search Linux/baseline/design-f36-narrow-dark` (36/36 + gates).
- Live 60fps motion evidence (no `SEARCH_BENCH`): `/home/alex/Search Linux/baseline/motion-evidence-f36/report.json`.
- Real-web HTTPS smoke: `https://example.com/` (917 chars), `https://en.wikipedia.org/wiki/Web_browser` (20,000 chars), `https://github.com/driceroland/Search` (17,204 chars).
