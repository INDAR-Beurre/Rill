# The loop, so it can be run again

One iteration is four moves. Nothing here is a one-off.

1. **Shoot.** `SHOT_SIZE=1500x917 tools/shots.sh council/round-N`, then
   `SHOT_DARK=1 ...` for the dark set. Real window captures, flattened onto the
   capture's own size, at the reference window's size. Never reuse an older
   round's pictures: the scores refer to a snapshot.
2. **Score.** Twenty critics: sixteen `reviewer` children, one facet each, and
   four Gemini 4 Hydrogen (medium) for fidelity, first impression, layout and
   colour. Each returns SCORE / SEEN / WHY / GAPS / EVIDENCE and nothing else.
   Scores go in `council/round-N/scores.json`.
3. **Fix.** Every facet below the floor gets a fix, not a caveat. Each fix needs
   a check: a test, a stress scenario, or a pixel that can be probed. Reject a
   fix the supervisor cannot build.
3. **Verify.** `cargo test`, `python3 tools/stress.py`, and the warning count
   (`cargo build --release | grep -c warning`, which must be 0). A round that
   does not pass all three is not a round.
4. **Record and push.** The memo's table gets the new row; the scores file is
   committed with the evidence that earned it.

## The gates a facet must pass before it can be called done

- its score is at least the target minus one;
- a critic has seen it in a screenshot from this round;
- the behaviour it describes has a test or a stress check behind it.

## Where the loop stands

| round | meant | best facets | blockers |
|---|---|---|---|
| 1 | evidence | - | the shot harness lied |
| 2 | capture fixed | fidelity 7 | panels, dark, field |
| 3 | real captures | fidelity 8 | field, panels, code |
| 4 | reference size | fidelity 8 | field, panels, code, keyboard |
| 5 | control skin, keys, filters | fidelity 9, impression 8, field 8 | keyboard 4, code 6, memory 6 |

Long-run discipline: a round with a failing stress check or a warning is
reopened before the next round starts, and a facet is never raised by argument.
