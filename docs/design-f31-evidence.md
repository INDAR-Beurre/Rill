# Design F31 evidence

## Fixes
- Window controls remain visible in the sidebar band when the optional toolbar is hidden.
- New tab is a quiet row in the tab column rather than a boxed primary button.
- The active tab is quiet (no border/inset accent) but keeps accessibility state.
- Folded rail glyph is shown; Peek scrim is removed on `clear()`/`clear_for()`.
- Search rank prefers open tabs with matching title/path over generic engine search.
- Offline errors are recognized by "no network"; the dark CTA keeps its button styling.
- Pinned grid starts at the column edge.

## Verification
- `cargo build --release`: zero warnings.
- `cargo test --release`: 305 passed, 0 failed, 1 ignored.
- `tools/test_ui.py`: passed.
- `tools/stress.py`: 142 passed, 0 failed.
- `tools/check_shield.py`: 15,775 rules accepted.
- `tools/extensions.py`: passed.
- `tools/compare.py --all`: passed.
- 36/36 light and dark states at 1500×918; 36/36 narrow states in each theme at 900×1001.

Council will judge these captures against Chrome, Firefox and Zen. No score is inferred from tool success.
