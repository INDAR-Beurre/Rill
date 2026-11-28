# Rill Public Website Handoff

## Scope and Branch
- Branch: `t3/build-rill-download-site` (Review #2 polish over `c1ff24c4aadf29f3da0bedfc3aa6c8c915ec3b0c`, base `main` `35b9a62`)
- Checkout path: `/home/alex/.t3/worktrees/Search Linux/t3-5b753195-4d306a779fb5a4f2/search-linux`
- Owned paths only: `site/` and `.github/workflows/site.yml` (zero Rust, packaging, README, or `main` changes).

## Review #2 Repairs (H1, H2, M1-M4, L1-L4)
1. **H1 — Matched Light + Dark Showcase Assets ([site/app.js](app.js), [site/index.html](index.html), `site/assets/`)**:
   - Limited Showcase tabs strictly to distinct, genuine matched Light + Dark GTK 3 capture pairs from `docs/screenshots/rill-f48/`: **Top tabs** (`top-tabs-light/dark.png`), **Privacy & shortcuts** (`setup-shortcuts-light/dark.png`), and **First-run welcome** (`introduction-light/dark.png`), removing unpaired single-theme light captures that flashed white in Warm charcoal dark mode and eliminating silent step-swapping under `Setup`.
2. **H2 — Mobile Showcase Screenshot Visibility ([site/index.html](index.html), [site/styles.css](styles.css))**:
   - Moved `<dl class="detail-ledger">` out of `.showcase-controls` into its own CSS grid item (`grid-column: 1; grid-row: 2;` on desktop, natural stack below `#showcase-tabpanel` on `<= 900px` mobile) so tapping any tab on `390x844` immediately updates and displays the screenshot directly underneath the tabs.
3. **M1 — Hero Composition, Headline & Bayer Horizon ([site/index.html](index.html), [site/styles.css](styles.css), [site/app.js](app.js))**:
   - Tightened `<h1>` to `"A quiet browser for Linux."` (`h1Height: 64px` single-line at `1440px`, balanced 2-line wrap on `390px` with `text-wrap: balance`) and `.hero-summary` to 13 balanced words.
   - Replaced the one-sided right-edge dither strip with a symmetric lower-horizon Bayer ordered-dither band anchored behind `#hero-frame` (echoing Rill's start page while keeping header, headline, summary, and `#hero-release-summary` on clean ground), with solid `--ground` backing on `#hero-caption`.
4. **M2 — Canonical `INDAR-Beurre/Rill` URLs & Case-Insensitive Matching ([site/release-core.mjs](release-core.mjs), [site/index.html](index.html))**:
   - Updated `API_URL`, `RELEASES_FALLBACK_URL`, `SOURCE_BUILD_URL`, and all 14 static links in `site/index.html` to `https://github.com/INDAR-Beurre/Rill`.
   - Updated `isTrustedGitHubUrl()` to match owner/repo prefixes case-insensitively while preserving strict HTTPS, `github.com`, no-credential/port/query/fragment, and no-traversal rules.
5. **M3 — Single-Option Readouts, Timeless Copy & `<pre>` Keyboard Access ([site/index.html](index.html), [site/styles.css](styles.css), [site/app.js](app.js), [site/release-core.mjs](release-core.mjs))**:
   - When `arches.length <= 1` or `matching.length <= 1` (e.g. `v1.1.0`), renders clean non-dropdown `.field-readout` boxes instead of inert 1-item `<select>` chevrons, automatically upgrading to `<select>` when multiple options exist.
   - Removed static `"1.1.0"` and `"for 1.2 is in progress"` strings from `#hero-caption` and `#limits` (kept `1.1.0` `glibc 2.44+` and `1.2` notes attached dynamically to resolved `1.1.x` releases inside `getSystemRequirements()`).
   - Added `tabindex="0"` and `aria-label` to `<pre>` blocks and wrapped long `printf ... | sha256sum -c -` pipelines across two indented lines.
6. **M4 — Sticker Sheet SVG Upgrade ([site/assets/rill-sticker-sheet.svg](assets/rill-sticker-sheet.svg))**:
   - Replaced Sticker 5's sparse bottom dot strip with a continuous 8x8 Bayer flowing wave on the right half (`Rust & GTK 3 / Native WebKitGTK browser`) and redesigned Sticker 6 as an archival split-badge typographic seal (`Rill design / Calm by default.`).
7. **L1–L4 Polish**:
   - **L1**: Accented `.view-detail` with a `2px solid var(--accent)` left rule and wider vertical spacing before `.detail-ledger`.
   - **L2**: Flattened `.fallback-box` to match `.terminal-section` hairline framing (no nested bordered rectangle).
   - **L3**: `buildInstallCommands` returns `hasVerification`; when no checksum sidecar or API digest exists, updates `#terminal-title` and note with an explicit unverified warning.
   - **L4**: Removed `aria-live="polite"` from `#release-meta` to prevent unprompted initial-load live-region chatter.

## Verification Performed
- `node --check site/release-core.mjs site/app.js`: exit `0`.
- `TMPDIR=/home/alex/.cache node --test site/tests/release-core.test.mjs`: **12/12 passing** (including real `/bin/sh` execution for `SITE-011`, `SITE-012`, `SITE-013`, static `SITE-014/M3`, case-insensitive `INDAR-Beurre/rill` URL check, and `L3` unverified-asset warning).
- `node /home/alex/.agents/skills/kill-ai-slop/scripts/scan.mjs site`: 4 intentional hits only (`underline` on links, `--font-mono` for CLI commands/SHA-256 digests).
- Rendered captures (`WebKit2 4.1` / `GTK 3.24` under `xvfb-run` at `/search-linux/` subpath `1440x900` Light/Dark + `firefox --headless` `390x844` with `TMPDIR=/home/alex/.cache`): zero broken images, `heroFits900: true` (`heroCaptionBottom: 870px`), single-line `<h1>` (`64px`), static readouts on `v1.1.0`, and verified matched dark PNGs across all 3 Showcase tabs.

## Deployment Note & Blockers
- GitHub Pages host settings remain untouched until final publication by root; [.github/workflows/site.yml](../.github/workflows/site.yml) prepares `build` and `deploy` (`pages: write`, `id-token: write`, `environment: github-pages`).
- Raster `gpt-image-2` 404 and `T3 preview_status` (`available: false`) blockers unchanged.
