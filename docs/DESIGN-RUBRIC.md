# Design council rubric

This rubric is the canonical scorecard for the browser design goal. Each seat
scores every facet from 1 to 10; a facet is complete only at **10/10**. Scores
must be tied to a fresh screenshot, a reproducible check, or an inspected
source fact. Platform gaps outside the goal boundary are recorded separately
and must not be silently converted into design defects.

| # | Facet | A 10/10 means |
| --- | --- | --- |
| 1 | Visual identity | One coherent editorial language across shell, tabs, pages, panels, light and dark; no borrowed or generic card-kit look. |
| 2 | Browser shell | Toolbar, sidebar, content, overlays and window controls align and nest cleanly; every shell state is represented. |
| 3 | Sidebar | Tab list, folded column, section labels and bottom actions are legible and balanced at wide and narrow widths. |
| 4 | Pinned tabs | Pinned state is visibly distinct, ordered, and does not crowd open tabs or controls. |
| 5 | Omnibox | Address, security state, grouping, shortcuts and suggestion states are clear, unclipped and visually stable. |
| 6 | Start page | Typography, spacing, bookmarks, history, shield summary and search field form a deliberate editorial composition. |
| 7 | Reader typography | Reading view has a calm, readable measure, hierarchy and contrast in both themes. |
| 8 | Reader size | 14/18/22px steps each remain usable and clip-free. |
| 9 | Find bar | Match count and controls remain visible and never collide with page content or panel controls. |
| 10 | Overlay panels | All six panels have clear rails, search fields, rows, scrollbars and empty/filled states. |
| 11 | Settings | Searchable sections, theme previews and compact reflow keep headings and controls attached to their content. |
| 12 | Library | Bookmarks/history/downloads states are visually consistent and usable when empty and filled. |
| 13 | Extensions/shortcuts | Extension state and shortcut reference are represented with explicit, verifiable evidence. |
| 14 | Error pages | DNS, TLS, offline and HTTP failures explain the problem and recovery action without visual noise. |
| 15 | Site info | Site card and Peek states are framed, legible and visually distinct from the page. |
| 16 | Dark mode | Charcoal surfaces, text, borders, scrollbars and scrims retain contrast and warmth; no white-on-white regressions. |
| 17 | Responsive | Full-shell captures at wide and narrow widths show no clipping, overlap, orphaned headings or unusable controls. |
| 18 | Motion | Panel, fold, omnibox, find, Peek and site-card transitions are smooth, bounded, and respect reduced-motion settings. |

## Evidence rules

- Use `tools/shots.sh` for real browser screenshots and keep the manifest with each round.
- Use `tools/test_ui.py` for geometry, clipping and keyboard evidence.
- Use `tools/stress.py` for motion gaps and interaction stability.
- Record exact scores, evidence paths, and the minimal fix for every sub-10 facet.
- Never average away a sub-10 score or claim a screenshot that was not captured.
