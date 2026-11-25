# F28 design evidence

## Shell fidelity change

F28 changes the default from a visible browser toolbar to the quiet reference-style shell:
- the toolbar is hidden until `Ctrl Shift S`, the Settings switch, or the menu toggle asks for it;
- the sidebar is 240px instead of 264px;
- tab rows are single-line 34px instead of 48px two-line;
- section labels, the tab count, and a divider between pins and open tabs are visible again;
- browser navigation remains functional through shortcuts and the revealable toolbar.

## Verification

- `cargo build --release`: zero warnings
- `cargo test --release`: 305 passed, 0 failed, 1 ignored
- `tools/test_ui.py`: passed, including explicit toolbar reveal/hide
- `tools/stress.py`: 142/142 passed, 185.8MB resident / 98.3MB own for one idle tab
- `tools/check_shield.py`: 15,775 rules accepted
- `tools/extensions.py`: all passed
- `tools/compare.py --all build/council/design-f28-light`: passed
- `tools/shots.sh`: 36/36 states in both light and dark at 1500x918

## Captures

- `build/council/design-f28-light/`
- `build/council/design-f28-dark/`

`build/` is intentionally gitignored; the screenshot manifests stay with the local evidence bundle.
