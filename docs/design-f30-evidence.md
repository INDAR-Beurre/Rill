# Design F30 evidence

F30 adds a quiet folded rail, fault-specific error icons/copy, Peek and Site Card scrims, exact-tab omnibox ranking, a real Settings scroller, and image-difference proof that Settings scrolls.

## Captures

- `build/council/design-f30-light/` — 36 states, 1500×918, light.
- `build/council/design-f30-dark/` — 36 states, 1500×918, dark.
- `/home/alex/Search Linux/baseline/design-f30-narrow/` — 900×1001 narrow captures in both themes.
- `/home/alex/Search Linux/baseline/motion-evidence-live/report.json` — live non-bench panel-change frames; proves a visible change, not animation smoothness.

## Verification

- `cargo build --release`: zero warnings.
- `cargo test --release`: 305 passed, 0 failed, 1 ignored.
- `tools/test_ui.py`: passed.
- `tools/stress.py`: 142 passed, 0 failed.
- `tools/check_shield.py`: 15,775 rules accepted.
- `tools/extensions.py`: passed.
- `tools/compare.py --all`: passed.
- Settings scroll gate: 29,750 changed pixels light, 29,235 dark.

## Council substitution

The documented Kimi seats are attempted first. If upstream content-safety screening rejects them, use Antigravity Gemini 4 Neon high and record the substitution in the round ledger.
