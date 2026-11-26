# F33 design evidence

Commit: `2c3cd29`

F33 is the reader-clarity and modal-restraint pass. It follows the strict F32 council, which scored every facet below 10 and specifically called out detached reader controls, a missing active size readout, harsh modal scrims, and weak dark field borders.

## Changes

- Reader font controls now sit 10px from the top of the article, show the active size (`14px`, `18px`, `22px`), and disable the control already at a boundary.
- Reader article top padding opens a clear 76px band for those controls.
- Reader copy uses the proper minus glyph in `A−`.
- Dark field borders move from `#898989` to `#707070` to pass the repo's contrast gate without becoming bright.
- Modal scrims are quieter: light `0.42 → 0.24`, dark `0.88 → 0.62`. The screenshot gate now checks uniform dimming in the 175–215 light range rather than asserting the old 148 grey.
- Reader assertions pin the active-size readout and three-step ladder.

## Checks

- `cargo build --release`: zero warnings.
- `cargo test`: 305 passed, 0 failed, 1 ignored.
- `tools/test_ui.py`: pass.
- `tools/stress.py`: 142/142.
- `tools/check_shield.py`, `tools/extensions.py`, `tools/compare.py --all`: pass.
- 36 light and 36 dark states at 1500×918: all gates pass.
- Fresh narrow evidence: `baseline/design-f33-narrow/{light,dark}-900x1001`, 4 states per theme.
- Real-web smoke from F32 still covers example.com, Wikipedia, and the reference GitHub repository; F33 changes are limited to reader chrome, scrim colour, and dark field borders.

Evidence roots:

- `build/council/design-f33-light` (in the isolated worktree used for capture)
- `build/council/design-f33-dark` (same)
- `/home/alex/Search Linux/baseline/design-f33-narrow`
- `/home/alex/Search Linux/baseline/f33-real-web-report.md` (if generated for this commit)
