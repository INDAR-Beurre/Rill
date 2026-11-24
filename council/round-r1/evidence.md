# Council round R1 evidence

This round reviewed the archived Linux build after extraction and after the
screenshot harness was repaired. It is the first scorecard for the current
goal; the pre-existing round-76 work is preserved only as historical context.

## Evidence

- Fresh source checkout: commit `a3ad90ec18c12ef7693b338d9fd550984223ef2a`
- Original archive SHA256: `5b3bd6ecd69ac25e44349c02ca12eb73b3c7e5820a1eaa4848bd227ac9e21758`
- Build: `cargo build --release`, zero warnings
- Tests: 304 passed, 0 failed, 1 ignored (documented live-network test)
- Shields: 15,775 rules compile; `tools/check_shield.py` passes
- Extension tests: `tools/extensions.py` passes
- Screenshots: 29 light and 29 dark states captured from the real app surface
- Stress: 140/142 in the original X11 screen; two failures were harness limits
  (exact resize beyond the X screen, and no window manager for maximise)

## Scores

All scores are out of 10. Seat 1 produced no score because its named Kimi
delegate failed with HTTP 400 and its retry failed before returning a result.
Seats 2 and 3 were re-seated on Antigravity `gemini-4-neon` (high) as required
by the fallback rule; this substitution is recorded here.

| Facet | Seat 1 | Seat 2 | Seat 3 |
| --- | --- | --- | --- |
| Visual identity | no score | 8 | 7 |
| Browser shell | no score | 6 | 6 |
| Sidebar | no score | 7 | 7 |
| Pinned tabs | no score | 7 | 7 |
| Omnibox | no score | 8 | 8 |
| Start page | no score | 8 | 8 |
| Reader typography | no score | 8 | 8 |
| Reader size | no score | 8 | 8 |
| Find bar | no score | 5 | 5 |
| Overlay panels | no score | 3 | 4 |
| Settings | no score | 2 | 3 |
| Library | no score | 5 | 8 |
| Extensions/shortcuts | no score | 4 | 4 |
| Error pages | no score | 7 | 8 |
| Site info | no score | 7 | 7 |
| Dark mode | no score | 6 | 7 |
| Responsive | no score | 3 | 4 |
| Motion | no score | 7 | 8 |
| Memory | no score | 6 | 6 |
| Verification rigor | no score | 4 | 5 |
| Mean | no score | 5.95 | 6.35 |

## Why the scores are below the bar

- Settings reopened from cache without its rows and compact mode collapsed the
  scrolling viewport to 55px.
- The window could not shrink below the size pinned by overlay size requests.
- The find match counter and panel body clipped at narrow widths.
- The bench panel-focus probe used the ancestor relationship backwards.
- Snapshots did not include a live-web smoke run, and the stress report did not
  explain two environment-dependent failures.
- Documentation still carried older feature/status claims.

## Gaps fixed before R2

The corresponding fixes are committed in this repository before the R2 review:
window/layer request release and Xvfb-based resize checks; Settings rebuild
instead of broken cache reuse; useful compact panel height; wider find counter;
correct focus descendant test; bounded main-loop/settle loops; relative dark
scrollbar pixel check; and full-screen screenshot of the real app surface.

The two environment-dependent stress failures were not papered over: the
production harness now runs under Xvfb with Openbox, where both resize and
maximise are real and all 142 checks pass.

Council review rule: a fresh review after these fixes is a new round (R2).
