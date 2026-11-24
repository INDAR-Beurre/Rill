# Final verification evidence

This directory contains 29 light and 29 dark full-window captures from the
final release binary, plus the stress and live-web reports. Light captures are
`NN-name.png`; dark captures are `dark-NN-name.png`.

## Build and static checks

- `cargo build --release` RC=0, zero warnings
- `cargo test --release`: 304 passed, 0 failed, 1 ignored
- `tools/check_shield.py`: 15,775 rules compile
- `tools/extensions.py`: all checks pass
- `tools/compare.py --all`: all structural checks pass

## Runtime checks

- `tools/test_ui.py` under Xvfb: 100% of UI checks pass, no GTK criticals and
  no invalid widget allocations; log `baseline/test_ui-r18.log`.
- `tools/stress.py` under Xvfb + Openbox: 142/142, 0 failures; one idle tab
  203.6 MB resident, 98.2 MB browser-owned; report `stress-report.json`.
- Live-web smoke: example.com, Wikipedia's Web browser article, and the
  reference GitHub repository all rendered real remote content; log
  `live-web-smoke.log`.
- 29/29 light and 29/29 dark states pass the screenshot manifest and pixel
  gates; both sets are 1500×917 captures taken with `SHOT_BACKEND=x11` and
  `SEARCH_SHOTS=bench` to avoid the compositor's 2× WebKit DPR mismatch.

## Honest limits

- The original ~100 MB total-RAM goal remains unmet under WebKitGTK; this build
  is materially below the archived 218.6 MB sample but still uses 203.6 MB RSS
  for one idle tab.
- No cloud sync, accounts, built-in password manager, multi-window, DRM/EME,
  macOS Spaces/gestures, Handoff, or keychain integration. These are documented
  platform decisions, not hidden gaps.
- The one ignored Rust test requires live DuckDuckGo availability and is
  documented in `docs/TESTING.md` with its explicit run command.
