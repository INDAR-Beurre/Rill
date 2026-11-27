# Design F38 evidence

Follows the three-seat Design F37 council (`baseline/council-f37-summary.md`, 35/60 scores `>= 9`, 25 below `9`) and closes every verified security, privacy, UI, motion, packaging, and documentation finding from F37 with unified frozen binary provenance.

## Frozen Provenance

- **Repository**: `/home/alex/Search Linux/work/search-linux`
- **Base `main` HEAD**: `558c206d04c2aef2c030ba9a922b82d98d9ee25d` (working tree with F38 edits; no commits/branches/pushes)
- **Deterministic Provenance Manifest & Verifier** (external to repo app/tool code):
  - Manifest: `/home/alex/Search Linux/baseline/f38-provenance-manifest.json` (SHA-256 `02b406f1f5d506aa6a2d419a95a3f96e565051ca50aaf485dd9f4cdb45bb4faf`)
  - Verifier: `/home/alex/Search Linux/baseline/f38_verify_provenance.py` (SHA-256 `c6ead32c25aabf0a7626ae384871ae50c103cca659c22a5813e3f26cab79e7c2`)
  - Reproduce check: `python3 "/home/alex/Search Linux/baseline/f38_verify_provenance.py" --check "/home/alex/Search Linux/baseline/f38-provenance-manifest.json"`
- **Release binary (`target/release/search`)**:
  - **SHA-256**: `cc25f2e5f2c1a44d80314a8c429b9ba8c62082e0529ca4953c49129dba5db5e2`
  - **Size**: `4,273,288` bytes (`4.08 MiB`)
- **Working-tree `git diff HEAD` fingerprints (run from repo root, SHA-256)**:
  - `git diff HEAD -- src/ assets/ | sha256sum`: `370171e71a9da83ba888f36a4aef784522da19e48c0259fa0fb6d3dc1e30c7d4`
  - `git diff HEAD -- tools/ packaging/ | sha256sum` (post-Stage-7a final, incl. `packaging/README.md`): `4c040a4ec2023f524eeffd07e2dbe4b227122890f885f8c4462cd4dcc733c1df`
  - `git diff HEAD -- tools/ packaging/ ':!packaging/README.md' | sha256sum` (executable scripts & AppImage payload only): `8deceafb3ee940316062586c4ab37f92abd93b8039646b2feb42ab6fb2362e9f`
  - `git diff HEAD -- packaging/README.md | sha256sum` (packaging docs only): `5aca31ef0c72e0254aca38e9fc76fb2549cd26c849e9bd3141769e0772ae91d4`
  - `git diff HEAD -- Cargo.toml Cargo.lock | sha256sum` (empty diff): `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- **Deterministic tree fingerprints (tracked + untracked, explicit framing)**:
  - **Framing A (`relpath_nul_bytes_nul_sha256`)**: files sorted by UTF-8 repo-relative POSIX path; each entry feeds `relpath.encode("utf-8") + b"\0" + file_bytes + b"\0"` into SHA-256:
    - `src/*.rs` (36 files, incl. untracked `src/f38_core_security_tests.rs`): `aa79a5aed6976b6904497f41ce0919b052f3d1589a0bba7a2e8a7e9f5c775541`
    - `assets/**` (5 regular files): `3ad7955f301e1dbeb2138f7d294e58f5e8ce122e51583390f0b32a3085c3f1f7`
    - `tools/**` + `packaging/**` excluding `__pycache__` (24 files, incl. untracked `tools/live_motion.py`, `tools/demo/office.html`, `packaging/README.md`): `e1a1b599fa249b4a5bea8842685ef88c75f407117f1f7a2eb498b63eb9fd9c4a`
    - `tools/**` + `packaging/**` excluding `packaging/README.md` and `__pycache__` (23 executable/payload files): `1a35acc4d8a02b7d8d7f4a8b1f46c9faf3e3882863de29456e04b72d4d4ff138`
  - **Framing B (`basename_nul_bytes_sha256`, used by `tools/live_motion.py:74-78`)**: files sorted by POSIX path; each entry feeds `basename.encode("utf-8") + b"\0" + file_bytes` into SHA-256:
    - `src/*.rs` (exact same 36 files as Framing A above): `b9c2411c0eef0afc874ca83e76b38dbf622dd3efb6baac79da8a0cc46c8cdf0e` (matches `baseline/motion-evidence-f38/report.json` `provenance.source_tree_sha256`)
  - **Untracked code/test files**:
    - `src/f38_core_security_tests.rs` (`30,990` B): `6c7e35197e7cc92914f56f77f6d0ff89c87f095c17551641d5b7f60a734f07cc`
    - `tools/live_motion.py` (`22,006` B): `458414549e01e13f689e14288acbceaec684285c723a8f81704a63f299df2f1b`
- **Historical pre-Stage-7a reconciliation (`baseline/f38-source-provenance.json`, captured `2026-11-26 18:58:43 +0100`)**:
  - `tools_packaging_diff_sha256` (`8a48edea656c5f13d9dc800a38308e4f175c1d3d9353bfc8c22b2695350ad86b`) and `tools_packaging_tree_sha256` (`f6e7e04bc87cf9ee62e97a3c957f2d55872500d009561da3d5fa65815c5535bc`) in that archived pre-docs snapshot are **superseded** by `4c040a4e…` and `e1a1b599…` / `1a35acc4…` above. Root cause: Stage 7a updated `packaging/README.md` (alongside the other 6 repo docs in one batch at `19:26:45 +0100`) to record the final AppImage SHA-256 and stress numbers. Zero files in `src/`, `assets/`, `Cargo.*`, `tools/`, or executable/payload `packaging/` changed after the `18:56:58` binary build, and `tools/appimage.sh` never bundles `packaging/README.md`. See `baseline/f38-freeze-handoff.md`.
- **Packaged AppImage (`build/Search-redesign-x86_64.AppImage`)**:
  - **SHA-256**: `81e8853b28ded66ab825f243937b2cc12c4d7efa73af0bb8a474f9d17a2c006b` (`sha256sum -c`: **OK**)
  - **Size**: `131,963,384` bytes (`125.85 MiB`)
  - **Embedded `usr/bin/search` SHA-256**: `c51764a473d83a8bb8e0e325521c5848fd055126e2ce370bf2ea7c33da2f8a6b` (differs from `target/release/search` strictly due to `patchelf --set-rpath '$ORIGIN/../lib'` adding `RUNPATH: [$ORIGIN/../lib]`; historic `build/Search-1.0.2-x86_64.AppImage` untouched)

## Summary of F38 Changes

1. **Core & Security (`src/app.rs`, `src/reader.rs`, `src/js.rs`, `src/curtain.rs`, `src/extensions.rs`, `src/permissions.rs`, `src/web.rs`, `assets/reader.js`, `src/f38_core_security_tests.rs`)**:
   - **Private tab persistence isolation (H2)**: `App::set_title()` skips `Records::name_visit()` and `schedule_flush()` on private tabs; private `hide_selector()` / `unhide_all()` operate solely in ephemeral per-tab memory (`App::private_hidden`) and private `UserContentManager` sheets without touching `hidden.json` or normal tabs.
   - **Isolated Reader world & strict DOM sanitizer (H4)**: Moved `assets/reader.js` and `searchReader` script message handler into the isolated `"search-reader"` WebKit script world (`READER_WORLD`), replaced `innerHTML` re-parse with direct DOM node adoption and strict tag/attribute/URL allowlist sanitization, and eliminated all main-world globals.
   - **Per-host curtain scope (M2)**: Replaced wildcard `&[]` curtain sheets with host-scoped `allow_list` patterns and per-host stylesheet registry on shared and private `UserContentManager`s.
   - **Live folded window controls sync (H3)**: Wired `sync_folded_controls()` into animated `toggle_fold()`, kept the floating pill visible below the revealed toolbar (`top = toolbar_height.max(72) + 8 = 80px`), and hid it in fullscreen.
   - **TLS error classification, extension exclusions, add-on confirm focus (M3, M4, L1)**: Exhaustive WebKit TLS reason mapping, `exclude_matches` enforcement in `extensions::here_at()`, and default Cancel focus on unsigned add-on prompts.
2. **UI, Motion & Integration Fixes (`src/panels.rs`, `src/sitecard.rs`, `src/omnibox.rs`, `src/motion.rs`, `src/peek.rs`, `src/css.rs`, `src/bench.rs`, `tools/test_ui.py`, `tools/shots.sh`, `tools/live_motion.py`)**:
   - **Settings filter-clear & true bottom scroll**: `Panels::set_filter()` queues resize/refit on the inner Settings scroller; `bench.rs` `panelscroll`/`settle` wait for `vadjustment().upper()` stabilization; `panels.rs` skips redundant `fit_height()` refits when `fitted_width == card_width` so Settings open produces `0ms` main-loop drift in `stress.py`.
   - **Site Card inset, scrim & `Gtk-WARNING` fix**: Removed duplicate `clear_block.add(&clear_note)`, unified `measured_card_width` / `hidden_toolbar_left` to preserve `18px` right inset at wide and narrow widths, and added alpha-animated dismiss scrim (`site-card-scrim`).
   - **Honest Omnibox security chip**: Uses active-tab `failed` + `view.tls_info()` via `sitecard::security()`; shows `verified` lock only on valid live HTTPS, `insecure` shield on plain HTTP and failed/unknown TLS (`04d-omni-tls-failed`), neutral `destination` chip on typed unvisited hosts, and `none` on search queries (`04c-omni-ranking`).
   - **Filter-icon `glycin-svg` elimination**: Replaced named GTK `edit-find-symbolic` in `Panels::filter_field()` with Cairo `Icon::Search`, eliminating `glycin-svg` + 2 `bwrap` at idle and after History/Bookmarks.
   - **Peek 1px relayout jump fix (`src/peek.rs`)**: Aligned `peek::page_left()` with `app.ui.stack.translate_coordinates(&app.ui.overlay, 0, 0)` (including `GtkPaned`'s 1px divider) so `open()` and `relayout()` agree to the pixel, eliminating the 1px post-load jump caught by `live_motion.py` `peek-reduced`.
3. **Shield & Packaging (`src/shield.rs`, `tools/appimage.sh`, `packaging/AppRun`)**:
   - Zero-allocation JSON array rule counter reporting exact **15,775** rules (matching `tools/check_shield.py`) and terminal-only `finished` accounting.
   - Dynamic host `pkg-config` WebKitGTK helper discovery, optional `gdk-pixbuf-2.0`, mandatory Liberation Serif 4-face + SIL OFL 1.1 license verification, GStreamer `libgstautodetect.so` + `libgstaudiofx.so` + LGPL license verification, and historic `1.0.1`/`1.0.2` overwrite guard.

## Verification Results (All Against Frozen Binary `cc25f2e5f2c1…`)

| Suite | Command | Result | Key Metrics / Evidence |
| --- | --- | --- | --- |
| Release build | `cargo build --release` | **PASS** (exit 0) | 0 warnings, `4,273,288` bytes, SHA-256 `cc25f2e5f2c1a44d80314a8c429b9ba8c62082e0529ca4953c49129dba5db5e2` |
| Rust unit tests | `cargo test --release` | **PASS** (exit 0) | **324 passed, 0 failed, 1 ignored (325 total)** (`suggest::tests::live_duckduckgo_answers_in_the_documented_shape`) |
| Shield compiler | `python3 tools/check_shield.py` | **PASS** (exit 0) | **15,775 rules** accepted (`hide-0`: 1, `hide-sites`: 1,774, `network`: 14,000), 0 lists rejected |
| Extension loader | `xvfb-run ... python3 tools/extensions.py` | **PASS** (exit 0) | **11/11 checks** passed |
| GTK UI walkthrough | `xvfb-run ... SEARCH_TEST_BACKEND=x11 python3 tools/test_ui.py` | **PASS** (exit 0) | All checks passed; 0 GTK criticals, 0 GTK warnings, 0 invalid allocations; Settings bottom scroll, Site Card 18px inset + `scrimclick`, Omnibox chip truth verified |
| Live-web HTTPS smoke | `f38_live_smoke.py` (`Xvfb + Openbox`) | **PASS** (exit 0) | `https://example.com` (`chipKind='verified'`, find `1 of 1`, Site Card open+dismiss), `https://en.wikipedia.org/wiki/WebKit` (Reader enter/exit, isolated world), private tab HTTPS, `https://expired.badssl.com/` (`chipKind='insecure'`, no bypass); 0 GTK warnings/criticals |
| Full stress suite | `xvfb-run ... openbox + SEARCH_STRESS_REPORT=build/stress-report-f38.json python3 tools/stress.py` | **PASS** (exit 0) | **150/150 passed (0 failed)**; idle 1 tab: **146.1 MB** warm / **146.7 MB** cold PSS, **61.6 MB** own, **404.7 MB** RSS, **3 procs**; `panel motion` `open 0ms, close 0ms`; all H2/M2/H4 live WebKit regressions passed |
| 4-way X11 captures | `SHOT_BACKEND=x11 ... bash tools/shots.sh` (×4) | **PASS** (exit 0) | **40/40 PNGs + 44/44 manifest entries, 0 failures** in all 4 sets (`1500×919` wide, `900×1002` narrow); true Settings bottom & Site Card 18px inset passed in all 4 |
| Gallery refresh | Copied 7 PNGs from `design-f38-{light,dark}` | **PASS** | All 7 `docs/screenshots/*.png` refreshed & directly visually inspected |
| Compare metrics | `python3 tools/compare.py --all build/council/design-f38-light` | **PASS** (exit 0) | All 40 wide light PNGs measured at `1500×919` |
| Live 60fps motion | `python3 tools/live_motion.py --out .../motion-evidence-f38` | **PASS** (exit 0) | **12/12 cases passed (`all_passed: true`)**, 0 GTK warnings, live X11 scrim click-outside `dismissed=true` |
| Live folded controls | `f38_folded_and_procs.py` (no `SEARCH_BENCH`, `Xvfb+Openbox`) | **PASS** (exit 0) | Animated `Ctrl+S` fold shows pill; `Ctrl+Shift+S` toolbar shifts pill below toolbar; `F11` fullscreen hides pill & restores cleanly; 0 GTK warnings |
| AppImage build + audit | `bash tools/appimage.sh` + extract audit | **PASS** (exit 0) | `build/Search-redesign-x86_64.AppImage` (`131,963,384` B), `sha256sum -c` **OK**, fonts + SIL OFL + GStreamer plugins + LGPL + WebKit helpers verified |
| Packaged AppImage QA | Native FUSE `SEARCH_BINARY=.../Search-redesign-x86_64.AppImage` | **PASS** (exit 0) | Full `test_ui.py` **PASS** (0 warnings); `stress.py --quick` **150/150 PASS** (`214.5 MB` PSS / `94.8 MB` own / `414.0 MB` RSS / `3` procs); local audio/WebAudio + live `https://github.com/` crash-free |
| Docs figure integrity | `python3 tools/report_figures.py --check` | **PASS** (exit 0) | All quoted figures in `docs/LIMITS.md` and `docs/DIFFERENCES.md` match `docs/benchmarks/stress-report-f38.json` |

## 4-Way Capture Manifest Summary (`tools/shots.sh`)

- **Wide Light (`build/council/design-f38-light`, `1500×919` X11)**: 44 entries, 0 failures; `[[194, 194, 194], [188, 188, 186]]`; `36678 pixels changed; scroll value=1783, upper=2123, page=340 (bottom_target=1783)`; `right_gap=18px (overlay=1500, card_at=[1017, 26], size=[465, 537]), scrim=True`; `sha256 cc25f2e5f2c1a44d80314a8c429b9ba8c62082e0529ca4953c49129dba5db5e2`.
- **Wide Dark (`build/council/design-f38-dark`, `1500×919` X11)**: 44 entries, 0 failures; `[[26, 24, 22], [26, 24, 22]]`; `36187 pixels changed; scroll value=1783, upper=2123, page=340 (bottom_target=1783)`; `right_gap=18px (overlay=1500, card_at=[1017, 26], size=[465, 537]), scrim=True`; `sha256 cc25f2e5f2c1a44d80314a8c429b9ba8c62082e0529ca4953c49129dba5db5e2`.
- **Narrow Light (`/home/alex/Search Linux/baseline/design-f38-narrow-light`, `900×1002` X11)**: 44 entries, 0 failures; `[[190, 190, 190], [188, 188, 186]]`; `34704 pixels changed; scroll value=1819, upper=2159, page=340 (bottom_target=1819)`; `right_gap=18px (overlay=900, card_at=[417, 26], size=[465, 537]), scrim=True`; `sha256 cc25f2e5f2c1a44d80314a8c429b9ba8c62082e0529ca4953c49129dba5db5e2`.
- **Narrow Dark (`/home/alex/Search Linux/baseline/design-f38-narrow-dark`, `900×1002` X11)**: 44 entries, 0 failures; `[[26, 24, 22], [26, 24, 22]]`; `34144 pixels changed; scroll value=1819, upper=2159, page=340 (bottom_target=1819)`; `right_gap=18px (overlay=900, card_at=[417, 26], size=[465, 537]), scrim=True`; `sha256 cc25f2e5f2c1a44d80314a8c429b9ba8c62082e0529ca4953c49129dba5db5e2`.

## Memory & Process Count Honesty

- **Original ~100 MB total-PSS goal remains UNMET** (F38 idle warm total PSS is **146.1 MB**, ~46 MB above 100 MB). The **230 MB WebKitGTK floor** enforced by `tools/stress.py` is a separate rebaselined floor (`target_pss_mb: 230`, `target_met: true`), not a claim that ~100 MB was met.
- **Process count audit (`baseline/f38-process-audit.json`)**:
  - Idle (1 loaded page): 3 processes, 170.3 MB PSS (`search` (73.5 MB), `WebKitNetworkPr` (31.3 MB), `WebKitWebProces` (65.5 MB))
  - After History panel: 3 processes, 170.3 MB PSS (`search` (73.5 MB), `WebKitNetworkPr` (31.3 MB), `WebKitWebProces` (65.5 MB)) (zero `glycin-svg`, zero `bwrap`)
  - After Bookmarks panel: 3 processes, 170.3 MB PSS (`search` (73.5 MB), `WebKitNetworkPr` (31.3 MB), `WebKitWebProces` (65.5 MB)) (zero `glycin-svg`, zero `bwrap`)
  - While Settings open: 6 processes, 179.1 MB PSS (`bwrap` (0.1 MB), `bwrap` (0.1 MB), `glycin-image-rs` (4.1 MB), `search` (78.0 MB), `WebKitNetworkPr` (31.3 MB), `WebKitWebProces` (65.5 MB))
  - After Settings closed: 6 processes, 179.1 MB PSS (`bwrap` (0.1 MB), `bwrap` (0.1 MB), `glycin-image-rs` (4.1 MB), `search` (78.0 MB), `WebKitNetworkPr` (31.3 MB), `WebKitWebProces` (65.5 MB)) (`GtkComboBoxText` / `GtkSwitch` triggers GTK3's system icon loader `glycin-image-rs` + 2 `bwrap`; we explicitly document this and do not claim elimination after opening Settings).

## Live 60fps Motion Assessment (`baseline/motion-evidence-f38/report.json`)

| Transition | Nominal Role | Normal Frames | Normal Wall Span | Reduced Frames | Reduced Wall Span | Warnings (N/R) | Gate |
| --- | --- | :---: | :---: | :---: | :---: | :---: | :---: |
| `panel` | ENTER 180ms | 6 (5 interm.) | 416.7 ms | 1 (0 interm.) | 0.0 ms | 0/0 | PASS |
| `fold` | LAYOUT 220ms | 12 (11 interm.) | 233.3 ms | 2 (1 interm.) | 16.7 ms | 0/0 | PASS |
| `omni` | ENTER 180ms | 5 (4 interm.) | 200.0 ms | 1 (0 interm.) | 0.0 ms | 0/0 | PASS |
| `find` | ENTER 180ms | 6 (5 interm.) | 116.7 ms | 1 (0 interm.) | 0.0 ms | 0/0 | PASS |
| `sitecard` | ENTER 180ms | 6 (5 interm.) | 316.7 ms | 1 (0 interm.) | 0.0 ms | 0/0 | PASS |
| `peek` | ENTER 180ms (chrome-only ROI) | 9 (8 interm.) | 266.7 ms | 1 (0 interm.) | 0.0 ms | 0/0 | PASS |

- **Live X11 scrim click-outside (`sitecard-normal`)**: `dismissed=True` (`diff_vs_open_px=1,192,905` vs `diff_vs_pre_px=3,518`).
- **Honest wall-time & software-rendering assessment**: `src/motion.rs` clamps per-tick `delta_us` to `MAX_FRAME_STEP_US = 33_333` (`33.3 ms`). On a hardware-accelerated 60Hz desktop (`~16.7 ms/frame`), ticks are un-clamped and complete at exact nominal durations (`180 ms` ENTER / `220 ms` LAYOUT). Under headless software `Xvfb`, full-window `1500×918` Cairo repaints with a dimmed scrim (`panel-normal` = 1.37M px/frame, `sitecard-normal` = 1.19M px/frame) take ~`50–83 ms` per frame (`12–20 fps`); clamping `delta_us` prevents a single software repaint from collapsing the entrance into 1 frame, at the explicit tradeoff of extending Xvfb wall-clock completion (`416.7 ms` for `panel-normal`, `316.7 ms` for `sitecard-normal`, `266.7 ms` for `peek-normal`, while smaller/narrower updates like `fold-normal` at `233.3 ms`, `omni-normal` at `200.0 ms`, and `find-normal` at `116.7 ms` stay near nominal). Changing 5–6 frames under Xvfb software rendering is therefore reported honestly as multi-frame interpolation bounded by software rasterization speed, not as proof of 60Hz hardware smoothness.
