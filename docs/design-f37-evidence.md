# Design F37 evidence

Follows the independent read-only F36 benchmark review (mean **9.06/10**, 8/18 at 10/10) and closes all ten sub-10 items plus a revealed-toolbar visibility regression caught during F37 screenshot verification.

## Changes

- **Revealed toolbar regression fix & proof (`src/app.rs`, `tools/shots.sh`)**: `sync_toolbar()` now calls `show_all()` on toolbar children when revealed (fixing the blank toolbar regression introduced when `set_no_show_all(true)` was added in F29). Added `01b-toolbar.png` to all four wide/narrow light/dark capture runs.
- **Omnibox security-state chip & proof (`src/omnibox.rs`, `tools/shots.sh`, `src/css.rs`)**: Added semantic transport icons (`Icon::Lock` for HTTPS, `Icon::ShieldAlert` with `.insecure` framing for HTTP) inside the clickable site chip beside the host, and added `04-omni-current.png` captured before clearing the field.
- **Settings height, panel y-anchor, scrollbar clearance, & compact rail (`src/panels.rs`, `src/css.rs`)**:
  - Raised `MAX_HEIGHT` (`640` → `740`) and `settings_scroll` (`260..380` → `340..480`) so filtered queries such as `"dark"` keep both `Appearance` and `Tabs & pages` together with the matching `Darken pages that stay white` toggle visible on screen at both `1500px` and `900px`.
  - Added `10px` right margin on all six scrollable panel lists to clear non-overlay scrollbars.
  - Aligned `Panels::open_panel_with` `slide_in_fixed` `y` anchor to `tallest` (matching `Panels::layout_card`) to prevent vertical jumps on shorter panels.
  - Switched `.panel.panel-compact button.panel-nav-item.on` to a bottom accent underline (`inset 0 -2px 0`) for the narrow horizontal tab rail.
- **Folded sidebar window controls (`src/ui.rs`, `src/app.rs`, `src/css.rs`)**: Added a quiet floating top-right window-controls pill (`-`, `□`, `×`) shown only when `folded && !toolbar_visible && !fullscreen`, keeping window controls reachable in `18-folded-column.png`.
- **Downloads progress/error & Extensions switch (`src/panels.rs`, `src/css.rs`)**: Added a slim terracotta `gtk::ProgressBar` for active downloads with known total size, semantic danger `.dl-failed` styling on failed transfers, and a `gtk::Switch` toggle alongside `Remove` for installed extensions.
- **Site Card close control (`src/sitecard.rs`)**: Added a quiet right-aligned `×` close button (`panelclose`) in the Site Card header.
- **Direct-draw scrim alpha & 6-transition live motion (`src/ui.rs`, `src/motion.rs`, `src/panels.rs`, `src/omnibox.rs`)**: Replaced full-screen `set_opacity()` on panel/omnibox scrims with direct Cairo alpha animation (`fade_alpha`) and captured all six transitions (`panel`, `fold`, `omni`, `find`, `sitecard`, `peek`) normal vs reduced at 60fps in `/home/alex/Search Linux/baseline/motion-evidence-f37/report.json`.

## Verification

- `cargo build --release`: zero warnings.
- `cargo test --release`: 305 passed, 0 failed, 1 ignored.
- `tools/check_shield.py`: 15,775 rules accepted, 0 rejected.
- `tools/extensions.py`: all extension checks passed.
- `tools/test_ui.py` (`SEARCH_TEST_BACKEND=x11`): passed (zero GTK criticals, zero invalid allocations).
- `tools/compare.py --all build/council/design-f37-light`: passed at `1500×918`.
- `tools/stress.py`: 142/142 passed (`188.1MB` resident, `98.3MB` own for one idle tab).
- Captures:
  - Wide light (`1500×918`): `build/council/design-f37-light` — 38/38 + pixel gates (`pixel light scrim`, `pixel settings scroll` 37,409px).
  - Wide dark (`1500×918`): `build/council/design-f37-dark` — 38/38 + pixel gates (`pixel dark corner [26,24,22]`, `pixel settings scroll` 37,241px).
  - Narrow light (`900×1001`): `/home/alex/Search Linux/baseline/design-f37-narrow-light` — 38/38 + pixel gates (`33,585px` scroll diff).
  - Narrow dark (`900×1001`): `/home/alex/Search Linux/baseline/design-f37-narrow-dark` — 38/38 + pixel gates (`33,280px` scroll diff).
  - Live 60fps 6-transition motion (no `SEARCH_BENCH`): `/home/alex/Search Linux/baseline/motion-evidence-f37/report.json`.
