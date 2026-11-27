# Design F39 evidence

Follows the three-seat Design F38 council (`baseline/council-f38-summary.md`, 53/60 scores `>= 9`) and closes every verified F38 finding across IPv6 Site Info / Curtain, Settings cold/warm stall & `glycin`/`bwrap` elimination, honest drift tracking, bounded first-paint-only motion, hardware Wayland GPU + X11 19-case motion verification, TLS `errorfocus` & overlay focus protection, and reproducible AppImage verification under a single frozen binary fingerprint.

## Frozen Provenance

- **Repository**: `/home/alex/Search Linux/work/search-linux`
- **Base `main` HEAD**: `558c206d04c2aef2c030ba9a922b82d98d9ee25d` (working tree with F39 edits; no commits/branches/pushes)
- **Deterministic Provenance Manifest & Verifier** (external to repo app/tool code):
  - Manifest: `/home/alex/Search Linux/baseline/f39-provenance-manifest.json` (SHA-256 `d7fdd214c4c68c315c191fdf20e18fd96d8b2d8e1df4b06e96746e5fa0258f3c`)
  - Verifier: `/home/alex/Search Linux/baseline/f39_verify_provenance.py` (SHA-256 `36af074471102d82032cd89bf923b66097975909d41829f442b036029628982f`)
  - Reproduce check: `python3 "/home/alex/Search Linux/baseline/f39_verify_provenance.py" --check "/home/alex/Search Linux/baseline/f39-provenance-manifest.json"`
- **Release binary (`target/release/search`)**:
  - **SHA-256**: `c2974efd71a0660e631806a05727b0be51d0815e58b3b0179420c007c665f624`
  - **Size**: `4,359,128` bytes (`4.16 MiB`)
- **Working-tree `git diff HEAD` fingerprints (run from repo root, SHA-256)**:
  - `git diff HEAD -- src/ assets/ | sha256sum`: `11b35617d37895f52700ef290d37fdbd57954973bbd2af4fdd70fd2ac9e33fc4`
  - `git diff HEAD -- tools/ packaging/ | sha256sum`: `f65e8d9817197e6ff18e559fe74ad4afac283f9405a6fa2c1121a89a85d931c7`
  - `git diff HEAD -- tools/ packaging/ ':!packaging/README.md' | sha256sum`: `112949a2d468fc7e409699856c660ca2e9f55b2c02994896f38d1124e24f40c5`
  - `git diff HEAD -- Cargo.toml Cargo.lock | sha256sum` (empty diff): `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- **Deterministic tree fingerprints (tracked + untracked, explicit framing)**:
  - **Framing A (`relpath_nul_bytes_nul_sha256`)**: files sorted by UTF-8 repo-relative POSIX path; each entry feeds `relpath.encode("utf-8") + b"\0" + file_bytes + b"\0"` into SHA-256:
    - `src/*.rs` (36 files): `a17110b87b7b334e5b40d5b07ec9bfd6785151c14208b6acc6f1ec3c83087ce9`
    - `assets/**` (5 regular files): `3ad7955f301e1dbeb2138f7d294e58f5e8ce122e51583390f0b32a3085c3f1f7`
    - `tools/**` + `packaging/**` excluding `__pycache__` (24 files): `643958bd530695f7839bb5a6ea0952d408812accf9d1f31372f36558218d47b6`
    - `tools/**` + `packaging/**` excluding `packaging/README.md` and `__pycache__` (23 files): `57b4712d95979dd1e28af4bf400fe68df74192c32aed08609e6601a726ae97ac`
  - **Framing B (`basename_nul_bytes_sha256`, used by `tools/live_motion.py` and `tools/motion_evidence.py`)**:
    - `src/*.rs` (exact same 36 files): `d5768545b76c9480126d9b244325a46e82f357cb03096ef6fd5c61615918d614` (matches both `baseline/f39-motion-wayland-gpu/report.json` and `baseline/f39-motion-x11/report.json`)
- **Packaged AppImage (`build/Search-redesign-x86_64.AppImage`)**:
  - **SHA-256**: `d5b00497b2a87d6f1941081a2f2d73fcd3835e92130b447e3fca2586c11214bb` (`sha256sum -c`: **OK**)
  - **Size**: `131,987,960` bytes (`125.87 MiB`)
  - **Embedded `usr/bin/search` SHA-256**: `c5cc0d19b7fad79d0882a283f8f5329616e67f70d3973e73a8a6fea4ea6569da` (differs from `target/release/search` strictly due to `patchelf --set-rpath '$ORIGIN/../lib'` adding `RUNPATH: [$ORIGIN/../lib]`; zero missing libs via `ldd`)
- **Preserved historical artifacts**:
  - `build/Search-1.0.2-x86_64.AppImage`: `7393bab265119d3a42d41982c0521ce0b553d9550775db0c5a5b488ecc41b4bc` (untouched)
  - Archived F38 binaries in `/home/alex/Search Linux/baseline/artifacts-f38/`: `search` (`cc25f2e5f2c1…`), `Search-redesign-x86_64.AppImage` (`81e8853b28de…`)
  - Historical `docs/design-f38-evidence.md`, `docs/benchmarks/stress-report-f38.json`, `docs/benchmarks/stress-report-f37.json` untouched.

## Summary of F39 Changes & Integration Repairs

1. **Core & Security (`src/shield.rs`, `src/curtain.rs`, `src/sitecard.rs`, `src/app.rs`, `src/omnibox.rs`, `src/keys.rs`, `src/peek.rs`, `src/f38_core_security_tests.rs`)**:
   - **IPv6 bracketed host support**: `shield::host_of`, `curtain::normalize_host`, `curtain::allow_list_for`, and Site Card handle IPv6 literals such as `http://[::1]:port/` end-to-end (live verified with Curtain hiding, Site Card display, and Curtain OFF/ON clearing).
   - **Curtain private-hide race & live pref clearing**: `HideOrigin` binds delayed `searchHide` messages to originating tab, host, privacy class, navigation epoch, and `UserContentManager`; `curtain::apply_curtain_pref()` immediately clears all live shared managers and immediate styles when toggled OFF and re-arms on ON.
   - **TLS `errorfocus` & overlay non-stealing**: Safe `Go back` button receives focus on TLS errors initially and on tab revisit via `should_focus_error_back`, exposed through the `errorfocus` bench verb. Integration testing (`tools/test_ui.py`) caught a real fall-through bug in `App::restore_page_focus()` where switching tabs while Settings was open could fall through `holder.child_focus(TabForward)` and steal focus to `error_back`; fixed so open panels, Site Card, and Peek always retain focus without falling through.
   - **Omnibox & folded controls**: Unsupported schemes (`ftp://`, `javascript:`, `data:`) never receive destination chips; folded window controls follow the unified column plan across sidebar, toolbar, and fullscreen.
   - **Cleanups**: Removed unused `ui::scrim`, removed stale `#[allow(dead_code)]` on `curtain::{clear_all_shared_managers, apply_curtain_pref}`, and renamed `m3_webkit_tls_reasons_are_security` to `m3_tls_checks` (0 compiler warnings).
2. **Settings Performance, Native Cairo Controls & Honest Drift Tracking (`src/panels.rs`, `src/css.rs`, `src/keys.rs`, `src/bench.rs`)**:
   - Eliminated Settings cold-open stall and `glycin-image-rs` + `bwrap` spawn by replacing `GtkSwitch` and `GtkComboBoxText` with native Cairo `pill-switch` (`GtkToggleButton`) and `engine-picker` (`GtkMenuButton` + `GtkPopover`), plus fixing redundant Pango wrapping and layout passes.
   - Added complete popover lifecycle & styling (`active_popover` tracking, `set_modal(true)`, `.chosen` highlight, start-page immediate refresh, focus return on close, `Escape` dismissing popover before panel, popover dismissal when opening omnibox/find/sitecard/another panel, and disabled OFF state on unsupported extensions).
   - Fixed `DriftTracker` (`src/bench.rs`) so `reset` starts observation immediately rather than masking stalls with a synthetic `0ms` reading; Settings cold/warm opens measure `69–90 ms` cold and `50–86 ms` warm (`gaps: []`, 50 ms ticker baseline) across wide light, wide dark, and narrow dark.
3. **Bounded First-Paint-Only Motion & Hardware Wayland GPU + X11 19-Case Harness (`src/motion.rs`, `tools/live_motion.py`, `tools/motion_evidence.py`)**:
   - Updated `src/motion.rs` to clamp only the first-paint setup tick (`MAX_FRAME_STEP_US`) while advancing subsequent ticks on real frame-clock deltas, keeping wall-clock durations bounded across hardware GPU and software Xvfb renderers.
   - Upgraded `tools/live_motion.py` with a persistent Wayland virtual input client (`zwp_virtual_keyboard_v1` + `zwlr_virtual_pointer_v1` via `libxkbcommon`), eliminating `wtype` ephemeral seat-capability churn that caused backdrop focus flicker and missed chords, enabling all **19/19** cases on both isolated Wayland GPU (including hardware Peek `Shift+Click`, exits, mid-flight reversals, and live Wayland scrim click-outside) and software `Xvfb + Openbox` (`wf-recorder -D` continuous 60fps capture + `ffprobe` PTS + unrecorded `GdkFrameClock` telemetry).
4. **Packaging & Harness Verification (`tools/appimage.sh`, `tools/test_ui.py`, `tools/stress.py`)**:
   - Auto-X11 selection when `WAYLAND_DISPLAY` is unset and `DISPLAY` is set verified in both `tools/test_ui.py` and `tools/stress.py --quick`; added Settings native control, unsupported-scheme chip, and TLS `errorfocus` live assertions to `tools/test_ui.py`.

## Verification Results (All Against Frozen Binary `c2974efd71a0…`)

| Suite | Command / Method | Result | Key Metrics / Evidence |
| --- | --- | --- | --- |
| Release build | `cargo build --release` | **PASS** (exit 0) | 0 warnings, `4,359,128` B, SHA-256 `c2974efd71a0660e631806a05727b0be51d0815e58b3b0179420c007c665f624` |
| Rust unit tests | `cargo test --release` | **PASS** (exit 0) | **337 passed, 0 failed, 1 ignored (338 total)** (`suggest::tests::live_duckduckgo_answers_in_the_documented_shape`) |
| Shield compiler | `python3 tools/check_shield.py` | **PASS** (exit 0) | **15,775 rules** accepted (`hide-0`: 1, `hide-sites`: 1,774, `network`: 14,000), 0 lists rejected |
| Extension loader | `xvfb-run ... python3 tools/extensions.py` | **PASS** (exit 0) | **11/11 checks** passed |
| Bench / UI self-tests | `python3 tools/test_bench.py` & `tools/test_ui.py --self-test` | **PASS** (exit 0) | `test_bench.py` **6/6 passed**; `test_ui.py --self-test` **PASS** |
| GTK UI walkthrough (source) | `xvfb-run ... env -u WAYLAND_DISPLAY -u SEARCH_TEST_BACKEND SEARCH_UI_OUTPUT=/tmp/f39-ui-source python3 tools/test_ui.py` | **PASS** (exit 0) | Auto-X11 verified; 0 GTK warnings/criticals; native Cairo controls, unsupported scheme chips, TLS `errorfocus` initial/revisit/overlay-non-steal verified |
| GTK UI walkthrough (AppImage) | `xvfb-run ... SEARCH_BINARY=.../Search-redesign-x86_64.AppImage python3 tools/test_ui.py` | **PASS** (exit 0) | Native FUSE **PASS**; 0 GTK warnings/criticals/allocations |
| Interactive controls & live smoke | `baseline/f39_verify_all_live.py` (Parts 1–6) | **PASS** (exit 0) | Settings cold/warm drift (`69–90ms` cold / `50–86ms` warm, `gaps=[]`), `motion_evidence.py`, source + AppImage 3-process audit (0 `glycin`/`bwrap`), interactive mouse+keyboard `pill-switch` & `engine-picker` popover light/dark, IPv6 `[::1]` Curtain + Site Card + Curtain OFF/ON clearing, AppImage live HTTPS (`example.com`, Wikipedia Reader, `expired.badssl.com` TLS + `errorfocus`, WebAudio + `github.com`), live non-`SEARCH_BENCH` folded controls; 0 GTK warnings |
| Full source stress suite | `xvfb-run ... openbox + SEARCH_STRESS_REPORT=docs/benchmarks/stress-report-f39.json python3 tools/stress.py` | **PASS** (exit 0) | **151/151 passed (0 failed)**; idle 1 tab: **182.9 MB** warm / **185.3 MB** cold PSS, **94.5 MB** own, **441.9 MB** RSS, **3 procs**; `panel motion` `open 73ms, close 50ms, 855 ticks, gaps []`; panels mean `27ms` / p95 `62ms` / worst `81ms`; tab switch mean `4.6ms` / p95 `13.5ms` / worst `15.7ms` |
| Packaged AppImage stress (×3) | 3 consecutive `tools/stress.py --quick` runs via native FUSE (Run #1 auto-X11 without `SEARCH_TEST_BACKEND`) | **PASS** (3/3, exit 0) | **151/151 passed** on all 3 runs (`213.9 / 196.2 / 213.6 MB` warm PSS, `94.1 / 76.5 / 93.8 MB` own, **3** procs, `panel motion` `open 57/55/73ms, close 50ms, gaps []`) |
| Wayland GPU 19-case motion | `python3 tools/live_motion.py --backend wayland-sway-gpu` | **PASS** (exit 0) | **19/19 passed (`all_passed: true`, `skipped_cases: []`)**, Intel Iris Xe `/dev/dri/renderD128` @ 60 Hz, 0 GTK warnings, live Wayland scrim click-outside `dismissed=true` |
| X11 software 19-case motion | `python3 tools/live_motion.py --backend x11-xvfb` | **PASS** (exit 0) | **19/19 passed (`all_passed: true`)**, `pre=0`, 0 GTK warnings, live X11 scrim click-outside `dismissed=true` |
| 4-way X11 captures & gallery | `SHOT_BACKEND=x11 ... bash tools/shots.sh` (×4) + 7 gallery PNGs + `tools/compare.py --all` | **PASS** (exit 0) | **40/40 PNGs + 44/44 manifest entries, 0 failures** in all 4 sets (`1500×919` wide, `900×1002` narrow); all 7 `docs/screenshots/*.png` refreshed & inspected |
| AppImage build + extract audit | `tools/appimage.sh --preflight && tools/appimage.sh` + `sha256sum -c` + extract audit | **PASS** (exit 0) | `131,987,960` B, SHA-256 `d5b00497b2a8…` (`OK`), `RUNPATH: [$ORIGIN/../lib]`, 0 missing libs, 4 Liberation Serif faces + SIL OFL, GStreamer `autodetect`+`audiofx` + LGPL, WebKit helpers verified |
| Docs figure & provenance checks | `tools/report_figures.py --check` & `baseline/f39_verify_provenance.py --check` | **PASS** (exit 0) | `LIMITS.md agrees with the report`; deterministic provenance manifest `ALL OK` |

## 4-Way Capture Manifest Summary (`tools/shots.sh`)

- **Wide Light (`build/council/design-f39-light`, `1500×919` X11)**: 44 entries, 0 failures; `[[194, 194, 194], [188, 188, 186]]`; `36510 pixels changed; scroll value=1783, upper=2123, page=340 (bottom_target=1783)`; `right_gap=18px (overlay=1500, card_at=[1017, 26], size=[465, 537]), scrim=True`; `sha256 c2974efd…`.
- **Wide Dark (`build/council/design-f39-dark`, `1500×919` X11)**: 44 entries, 0 failures; `[[26, 24, 22], [26, 24, 22]]`; `35758 pixels changed; scroll value=1783, upper=2123, page=340 (bottom_target=1783)`; `right_gap=18px (overlay=1500, card_at=[1017, 26], size=[465, 537]), scrim=True`; `sha256 c2974efd…`.
- **Narrow Light (`/home/alex/Search Linux/baseline/design-f39-narrow-light`, `900×1002` X11)**: 44 entries, 0 failures; `[[190, 190, 190], [188, 188, 186]]`; `34535 pixels changed; scroll value=1801, upper=2141, page=340 (bottom_target=1801)`; `right_gap=18px (overlay=900, card_at=[417, 26], size=[465, 537]), scrim=True`; `sha256 c2974efd…`.
- **Narrow Dark (`/home/alex/Search Linux/baseline/design-f39-narrow-dark`, `900×1002` X11)**: 44 entries, 0 failures; `[[26, 24, 22], [26, 24, 22]]`; `33739 pixels changed; scroll value=1801, upper=2141, page=340 (bottom_target=1801)`; `right_gap=18px (overlay=900, card_at=[417, 26], size=[465, 537]), scrim=True`; `sha256 c2974efd…`.

## Memory & Process Count Honesty

- **Original ~100 MB total-PSS goal remains UNMET** (F39 full-run idle warm total PSS is **182.9 MB**, ~83 MB above 100 MB). The **230 MB WebKitGTK floor** enforced by `tools/stress.py` is a separate rebaselined floor (`target_pss_mb: 230`, `target_met: true`), not a claim that ~100 MB was met.
- **Process count audit (`baseline/f39-process-audit.json`)**:
  - **Source binary**: 3 processes at idle (`174.0 MB`), after History (`174.0 MB`), after Bookmarks (`174.1 MB`), while Settings open (`174.7 MB`), and after Settings closed (`174.7 MB`) — `search`, `WebKitNetworkPr`, `WebKitWebProces`; **0 `glycin-image-rs`, 0 `glycin-svg`, 0 `bwrap`**.
  - **Packaged AppImage**: 3 processes at idle (`206.1 MB`), after History (`206.1 MB`), after Bookmarks (`206.1 MB`), while Settings open (`206.7 MB`), and after Settings closed (`206.7 MB`) — `search`, `WebKitNetworkPr`, `WebKitWebProces`; **0 `glycin-image-rs`, 0 `glycin-svg`, 0 `bwrap`**.

## Live 19-Case Motion Assessment (Wayland GPU & X11 Software)

- **Hardware Wayland GPU environment** (`baseline/f39-motion-wayland-gpu/report.json`): isolated headless `sway` (`wlroots`), `/dev/dri/renderD128`, EGL `iris`, `Mesa Intel(R) Iris(R) Xe Graphics (TGL GT2)`, `OpenGL ES 3.2 Mesa 26.2.6-arch1.1`, `HEADLESS-1 1500x918 @ 60.000000 Hz`, persistent Wayland `zwp_virtual_keyboard_v1` + `zwlr_virtual_pointer_v1`, `wf-recorder -D` continuous 60fps capture + unrecorded `GdkFrameClock` telemetry.
- **Honest renderer & capture caveat**: While the Sway Wayland compositor composites on Intel Iris Xe hardware GPU at 60 Hz (and unrecorded `GdkFrameClock` telemetry logs 7–14 ticks / ~60 Hz pacing on that compositor), GTK3 client widget rendering is always CPU Cairo into `wl_shm`, and `wf-recorder -D` performs CPU `zwlr_screencopy` readback + lossless x264 encoding, which lowers recorded frame cadence on full-window scrim paints compared to unrecorded display pacing. Both recorded frames and unrecorded telemetry ticks are reported side by side below.

| Case | Nominal Role | Wayland GPU Frames / Span (Unrec. Ticks) | X11 Xvfb Frames / Span | Warnings (W/X) | Gate |
| --- | --- | --- | --- | :---: | :---: |
| `panel-normal` | ENTER 180ms | 7 (6 interm.) / `200.0 ms` (`[11, 11]`) | 6 (5 interm.) / `233.3 ms` | 0/0 | PASS |
| `panel-reduced` | Instant | 1 (0 interm.) / `0.0 ms` | 1 (0 interm.) / `0.0 ms` | 0/0 | PASS |
| `fold-normal` | LAYOUT 220ms | 12 (11 interm.) / `183.3 ms` (`[11, 14]`) | 7 (6 interm.) / `116.7 ms` | 0/0 | PASS |
| `fold-reduced` | Instant | 2 (1 interm.) / `16.7 ms` | 1 (0 interm.) / `0.0 ms` | 0/0 | PASS |
| `omni-normal` | ENTER 180ms | 5 (4 interm.) / `166.7 ms` (`[8, 8]`) | 5 (4 interm.) / `166.7 ms` | 0/0 | PASS |
| `omni-reduced` | Instant | 1 (0 interm.) / `0.0 ms` | 1 (0 interm.) / `0.0 ms` | 0/0 | PASS |
| `find-normal` | ENTER 180ms | 4 (3 interm.) / `83.3 ms` (`[11]`) | 6 (5 interm.) / `116.7 ms` | 0/0 | PASS |
| `find-reduced` | Instant | 1 (0 interm.) / `0.0 ms` | 1 (0 interm.) / `0.0 ms` | 0/0 | PASS |
| `sitecard-normal` | ENTER 180ms | 7 (6 interm.) / `133.3 ms` (`[10, 10]`) | 6 (5 interm.) / `116.7 ms` | 0/0 | PASS |
| `sitecard-reduced` | Instant | 1 (0 interm.) / `0.0 ms` | 1 (0 interm.) / `0.0 ms` | 0/0 | PASS |
| `peek-normal` | ENTER 180ms (chrome ROI) | 5 (4 interm.) / `116.7 ms` (`[10, 10]`) | 5 (4 interm.) / `150.0 ms` | 0/0 | PASS |
| `peek-reduced` | Instant (chrome ROI) | 1 (0 interm.) / `0.0 ms` | 1 (0 interm.) / `0.0 ms` | 0/0 | PASS |
| `panel_exit-normal` | EXIT 130ms | 5 (4 interm.) / `116.7 ms` (`[8]`, end=0px) | 4 (3 interm.) / `116.7 ms` (end=0px) | 0/0 | PASS |
| `panel_exit-reduced` | Instant | 1 (0 interm.) / `0.0 ms` (end=0px) | 1 (0 interm.) / `0.0 ms` (end=0px) | 0/0 | PASS |
| `peek_exit-normal` | EXIT 130ms (chrome ROI) | 5 (4 interm.) / `100.0 ms` (`[7, 7]`, clean) | 5 (4 interm.) / `116.7 ms` (end=0px) | 0/0 | PASS |
| `peek_exit-reduced` | Instant (chrome ROI) | 1 (0 interm.) / `0.0 ms` (end=0px) | 1 (0 interm.) / `0.0 ms` (end=0px) | 0/0 | PASS |
| `panel_reverse-normal` | Mid-flight reversal | 7 (6 interm.) / `200.0 ms` (end=2px, clean) | 6 (5 interm.) / `233.3 ms` (end=0px) | 0/0 | PASS |
| `fold_reverse-normal` | Mid-flight reversal | 14 (13 interm.) / `216.7 ms` (end=0px, clean) | 13 (13 interm.) / `233.3 ms` (end=0px) | 0/0 | PASS |
| `peek_reverse-normal` | Mid-flight reversal | 8 (7 interm.) / `216.7 ms` (end=648px, clean) | 9 (8 interm.) / `383.3 ms` (end=0px) | 0/0 | PASS |

- **Live scrim click-outside (`sitecard-normal`)**: Wayland GPU `dismissed=True` (`diff_vs_open_px=1,191,295`, `diff_vs_pre_px=180`); X11 `dismissed=True` (`diff_vs_open_px=1,192,905`, `diff_vs_pre_px=3,541`).
