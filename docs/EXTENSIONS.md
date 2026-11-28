# Extensions & Add-on Compatibility

Rill uses **WebKitGTK 4.1**, not Gecko or Chromium. There is **no WebExtensions runtime** (`chrome.*` / `browser.*` APIs, background pages or service workers, `browser.storage.*`, `webRequestBlocking`, toolbar popups, or `nativeMessaging`).

## What actually works

- **Pure content scripts, CSS, and `options_ui`**: Add-ons whose work runs purely via DOM content scripts/CSS matched by host pattern can run on matching sites.
- **Built-in native features instead of heavy extensions**:
  - **Ad & tracker blocking**: Built directly into WebKitGTK via **15,775 compiled EasyList + EasyPrivacy rules** (`Settings → Blocker`), with no extension process or `webRequestBlocking` needed.
  - **Element hiding**: Per-site persistent DOM element hiding (`Ctrl Shift H`) runs in an isolated WebKit script world.
  - **Darkening bright pages**: Built into `Settings → Darken pages that stay white`.
  - **Passwords & autofill**: **Disabled by design.** Rill never saves credentials or autofills forms; password managers such as **Bitwarden**, **1Password**, and **KeePassXC-Browser** do **not** work as browser extensions here. Use a standalone desktop password manager and paste manually.

## Real-package verification (110 AMO `.xpi` files)

During F47 verification (`2026-11-27`), **110 actual SHA-verified `.xpi` packages** downloaded from `addons.mozilla.org` were tested against frozen binary `build/rill` (`1003a36e20d23700b2761647489f0f4241cd58d071434b7d20cd239ffe05fea8`). This repeats the F44 package set using disk-backed temporary storage, removing the host storage failures from that earlier run. This is distinct from the **108 synthetic loader fixtures** in [`tools/extension_matrix.py`](../tools/extension_matrix.py).

Full machine-readable per-package results (version, SHA-256, outcome, loader tier, and rejection/limitation notes; no `.xpi` binaries redistributed): [`docs/benchmarks/real-extensions-f47-summary.json`](benchmarks/real-extensions-f47-summary.json). The [F44 report](benchmarks/real-extensions-f44-summary.json) remains available as historical evidence; 22 of its 25 rejections were host storage failures rather than compatibility findings. These results describe the audited F47 binary, not a functional certification of later builds.

### Methodology & strict limits of that test

- Each `.xpi` was downloaded from official `addons.mozilla.org` URLs, verified against AMO's published SHA-256 hash, and passed serially to Rill's isolated `installext` loader with prior add-ons disabled.
- Local-page smoke checked only whether declared content scripts injected onto a local test page. **No vault, account sign-in, sync, popup UI, network blocking, or autofill workflow was tested.**
- Therefore, **106 installed packages means 106 installed + local-page probed only — NOT 106 functioning products.**

### Summary counts (`110` real packages)

| Metric | Count | Meaning |
|---|---:|---|
| Total tested | **110** | Real AMO `.xpi` packages, SHA-verified |
| `install_rejected` | **4** | Three exceeded the 16 MB per-file unpack cap; one exceeded 4,096 archive entries. No host storage failures. |
| `installed_and_local_page_probed` | **106** | Unpacked and inspected by the loader (local-page smoke only) |
| Loader tier: `unsupported` | **22** | Background-only (`runs: false`); nothing executes |
| Loader tier: `partial` | **83** | Content scripts/CSS load (`runs: true`), while background pages, `chrome.*`/`browser.*` APIs, storage, or native messaging fail |
| Loader tier: `content_capable` | **1** | Pure content-script add-on with empty `why_not` (`video-background-play-fix` 1.8.1) |

### Notable real-package outcomes

| Package | Version | Outcome | Loader tier | Honest note |
|---|---|---|---|---|
| **Video Background Play Fix** (`video-background-play-fix`) | `1.8.1` | Installed + local smoke | `content_capable` | Pure content script; no background/storage dependency flagged |
| **Stylus** (`styl-us`) | `2.4.19` | Installed + local smoke | `partial` | Content scripts/CSS load; editor, popup, and extension storage unavailable |
| **Dark Reader** (`darkreader`) | `4.9.135` | Installed + local smoke | `partial` | Background/storage APIs missing; full workflow unavailable (use built-in page darkener) |
| **SponsorBlock** (`sponsorblock`) | `6.1.7` | Installed + local smoke | `partial` | Content scripts load, background/storage APIs unavailable; skipping not verified |
| **Return YouTube Dislike** (`return-youtube-dislikes`) | `4.1.7` | Installed + local smoke | `partial` | Background messaging unavailable; counts not verified |
| **KeePassXC-Browser** (`keepassxc-browser`) | `1.10.4.1` | Installed + local smoke | `partial` | Background & `nativeMessaging` missing — **vault/autofill does not work** |
| **1VPN** (`1vpn`) | `4.2.3` | Installed + local smoke | `unsupported` | Background-only proxy/webRequest APIs — nothing runs |
| **1Password** (`1password-x-password-manager`) | `8.12.46.29` | Install rejected | `not_installed` | Rejects 17 MB `.wasm` (16 MB cap); even if unpacked, lacks `nativeMessaging`/storage |
| **Bitwarden** (`bitwarden-password-manager`) | `2026.11.0` | Installed + local smoke | `partial` | Background/storage APIs missing — **vault/autofill does not work** |
| **uBlock Origin** (`ublock-origin`) | `1.77.4` | Installed + local smoke | `partial` | Requires `webRequestBlocking`/background for blocking (use built-in Blocker) |
| **Wappalyzer** (`wappalyzer`) | `6.12.9` | Install rejected | `not_installed` | Archive holds 13,112 entries (4,096-entry unpack cap) |

## In-app stores, AMO catalog & security guards

- **Chrome Web Store entry point (honest disclosure)**: **Add-ons & Extensions** provides a discoverable **Chrome Web Store** link (`https://chromewebstore.google.com/`) alongside **Firefox Add-ons** (`https://addons.mozilla.org/firefox/extensions/`). Because Rill runs on WebKitGTK rather than Chromium, one-click installation from the Chrome Web Store is **not** supported. Users with a local `.crx`, `.xpi`, or unpacked extension folder can inspect it through **Add from this computer** (`Choose file…` / `Choose folder…`), which runs `extensions::inspect` and shows Rill's confirmation card (`Cancel` focused by default) before copying anything.
- **User-initiated only**: Opening **Add-ons & Extensions** shows offline curated listings and makes **zero** network calls until you submit a search or click **Add to Rill…** / **Inspect…** / **Open store**.
- **Strict host & redirect bounds**: Automatic store downloads allow only `https://addons.mozilla.org` and `https://addons.cdn.mozilla.net`, check every HTTP redirect hop against that allowlist, cap response streams in flight (`512 KiB` search JSON, `32 MiB` `.xpi`), and drop stale cancelled search responses.
- **Honest catalog wording**: Catalog search results display `Not verified` (permissions do not prove content-script support), `API-dependent`, or `Requires WebExtensions APIs`. Official `.xpi` entries that are not classified `Requires WebExtensions APIs` show **Add to Rill…**; unsupported entries show **Inspect…**. Every install still goes through Rill's explicit permission confirmation dialog (`Cancel` focused by default).
