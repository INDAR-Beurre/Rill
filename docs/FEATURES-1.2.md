# Rill 1.2 candidate improvements

This is a candidate backlog. None of these rows is a claim of implementation.
Builders must reject ideas already present, validate the underlying user need,
and record final status and evidence in their handoffs. Prefer a smaller set of
complete improvements to superficial additions made to reach a number.

Defaults remain minimal. Batch operations stay scoped to the chosen space and
respect pins, private tabs, tab limits, and active-tab invariants. Explicit
exports may include only the information the user selected. Private browsing
must never quietly enter persistent sessions, diagnostics, or regular history.

## Tabs, spaces, navigation, and sessions

| ID | Candidate | Acceptance focus |
| --- | --- | --- |
| T01 | Close tabs to the left | Preserve pins and tabs in other spaces |
| T02 | Close duplicate tabs | Keep chosen tab; separate private and regular pages |
| T03 | Close other tabs from this site | Match origin correctly and preserve pins |
| T04 | Move tab to the beginning | Stay within pinned/unpinned partition |
| T05 | Move tab to the end | Stay within current space and partition |
| T06 | Reload tabs in this space | Skip blank/asleep tabs and other spaces |
| T07 | Stop all loads in this space | Stop real loading views only |
| T08 | Mute other tabs in this space | Leave the chosen tab's mute state alone |
| T09 | Unmute all tabs in this space | Update live views and any saved tab mute state |
| T10 | Put a chosen background tab to sleep | Respect capture/audio and preserve recovery |
| T11 | Put eligible background tabs to sleep now | Act immediately without waiting for idle timeout |
| T12 | Keep a chosen tab awake | Visible, reversible state; consistent with sleep timer |
| T13 | Keep chosen sites awake | Exact bounded host rules with removal controls |
| T14 | Copy a background tab's address | Use the clicked tab, not the active tab |
| T15 | Copy page title and address | Plain-text output without credentials |
| T16 | Copy a Markdown page link | Escape title punctuation and unsafe URL characters |
| T17 | Copy all page addresses in this space | Visual order; omit blank/internal pages |
| T18 | Bookmark this space's pages | Deduplicate and explain what was saved |
| T19 | Recently closed tabs picker | Titles and addresses with individual restore actions |
| T20 | Restore a recently closed batch | Bound by tab limit; avoid private persistence |
| T21 | Sort unpinned tabs by title | Stable ordering within current space |
| T22 | Group unpinned tabs by site | Stable origin grouping, preserve active page |
| T23 | Reorder spaces | Persist stable IDs and handle Default explicitly |
| T24 | Duplicate a space's regular pages | Respect space/tab limits without private leakage |
| T25 | Rename the current space from its switcher | Inline, accessible, validated name editing |
| T26 | Move a tab into a newly created space | Atomic operation if limits prevent creation |
| T27 | Keyboard shortcut for next space | Advertise shortcut and skip no valid spaces |
| T28 | Keyboard shortcut for previous space | Wrap predictably without changing tab history |
| T29 | Search the space switcher | Keyboard navigation and useful empty state |
| T30 | Search tabs across spaces | Results identify space and preserve privacy boundary |
| T31 | Save a named regular-tab session | Explicit save; versioned bounded format |
| T32 | Open a saved session alongside current tabs | Show capacity errors without partial data loss |
| T33 | Replace a space from a saved session | Confirm scope and provide recovery |
| T34 | Export regular tab addresses to a text file | User-chosen path and atomic file write |
| T35 | Import an address list with preview | Validate schemes, deduplicate, enforce tab limit |
| T36 | Rename a saved session | Collision handling and persistence |
| T37 | Delete a saved session | Confirmation or undo; do not close live tabs |
| T38 | Choose a custom Home address | Validate scheme and preserve blank-home fallback |
| T39 | Choose a default page zoom | Clamp values; retain explicit per-site overrides |
| T40 | Choose new-tab placement | Adjacent to active or end, within current space |
| T41 | Choose startup behavior | Restore, blank page, or configured Home |
| T42 | Choose which tab activates after closing | Predictable left/right/recent behavior |
| T43 | Warn before a bulk tab close | Summarize actual affected pages; cancel is harmless |
| T44 | Protect pinned tabs from ordinary close actions | Reversible preference with explicit unpin path |
| T45 | Copy a link without known tracking parameters | Preserve meaningful query parameters and fragments |
| T46 | Search selected page text | Use selected engine without an implicit network request |
| T47 | Open a selected web address | Accept safe explicit URL text only |
| T48 | Open multiple clipboard addresses after review | Never execute clipboard text as a command |
| T49 | Save a page for offline reading | Native save dialog, actual saved document, safe names |
| T50 | Export reading-mode text | Explicit destination; preserve useful paragraph breaks |

## Library, settings, accessibility, and page tools

| ID | Candidate | Acceptance focus |
| --- | --- | --- |
| L01 | Edit a bookmark title | Persistence and immediate toolbar/start-page refresh |
| L02 | Edit a bookmark address | Scheme validation and duplicate resolution |
| L03 | Move a bookmark up | Update all bookmark surfaces in the same order |
| L04 | Move a bookmark down | Accessible action and clear boundary state |
| L05 | Sort bookmarks by title | Explicit action, stable case-insensitive order |
| L06 | Sort bookmarks by site | Correct host extraction and stable ordering |
| L07 | Export bookmarks as HTML | Standard escaping, native save dialog, atomic write |
| L08 | Import a bookmarks HTML file | Bounded parser, safe schemes, deduplication |
| L09 | Open a bookmark in a background tab | Preserve active page and honor tab limit |
| L10 | Open a bookmark privately | No regular history entry or unexpected profile data |
| L11 | Copy a bookmark address | Accessible action, no navigation |
| L12 | Undo bookmark removal | Restore original position; no stale undo mutation |
| L13 | Bookmark folders | Backward-compatible data model, simple flat default |
| L14 | Move bookmarks between folders | Update only selected bookmarks |
| L15 | Search bookmarks by folder and title | No missing results beyond rendering cap |
| L16 | Filter history to a chosen site | Clear active filter and reset path |
| L17 | Filter history by date range | Correct local date boundaries |
| L18 | Clear history from the last hour | Confirm scope and leave older entries alone |
| L19 | Clear today's history | Local-day behavior; explicit irreversible action |
| L20 | Remove a site's history | Exact site scope and cancellation |
| L21 | Export selected regular history | Opt-in, explicit destination, no private visits |
| L22 | Open a history item in a background tab | Preserve panel/filter and active tab |
| L23 | History results count and paging | Make capped results discoverable |
| L24 | Clear finished download records | Leave files and in-progress downloads untouched |
| L25 | Filter downloads by status | Active, finished, and failed states remain accurate |
| L26 | Copy a download's source address | Sanitized display; no automatic request |
| L27 | Copy a download's local path | Clipboard action separate from file opening |
| L28 | Retry a failed regular download | Retain source, prevent unsafe automatic overwrite |
| L29 | Reveal the actual downloaded file | Open the containing directory with safe URI handling |
| L30 | Mark missing downloaded files | Accurate state without deleting records silently |
| L31 | Choose a default download folder | Native folder chooser, persisted usable path |
| L32 | Ask where to save each download | Cancel leaves no partial file or false completion |
| L33 | Find in page with match-case option | Native WebKit matching; keyboard-accessible toggle |
| L34 | Find in page with whole-word option | Defined Unicode behavior and clear unavailable state |
| L35 | Start Find from selected page text | Bounded text, preserve active page |
| L36 | Reading-mode font size control | Persistent preference and immediate live application |
| L37 | Reading-mode text width control | Accessible measure across narrow and wide views |
| L38 | Reading-mode line-height control | Bounded values, no article clipping |
| L39 | Reading-mode font family choice | Small deliberate choice; no added image helpers |
| L40 | Reading-mode theme independent of chrome | Light/dark/automatic and correct contrast |
| L41 | Reduced-motion preference | Honor desktop preference with explicit override |
| L42 | Larger interface text preference | Live geometry, no clipped controls at small windows |
| L43 | Clear a single site's zoom override | Preserve other sites and private isolation |
| L44 | Change the tab sleep timeout | Bounded values, clear audio/capture exemption |
| L45 | Search settings with stable section links | Useful results and reliable keyboard focus |
| L46 | Reset only one settings section | Explicit scope; preserve unrelated preferences |
| L47 | Inspect runtime version information | Actual app/GTK/WebKit versions, copyable plain text |
| L48 | Copy privacy-safe diagnostics | No URLs, history, profile paths, credentials, or hostnames |
| L49 | Open the actual profile folder | Explicit action with clear compatibility explanation |
| L50 | Show update availability on request | Official release source, no startup network polling |

## Recording progress

For each accepted row, record source files, review findings and their resolution,
tests or real-input evidence, and final candidate revision/hash. Rejected and
deferred ideas stay visible with a reason. Count only reviewed, verified user
improvements in the final release notes.

## Baseline audit before implementation

The 1.1 source already provides pinned tabs, tab duplication, single-step tab
reordering, close-other and close-right actions, a last-closed-tab stack,
per-host zoom for the running session, and a title/address tab switcher.
Extending those behaviors must offer a new usable capability to count here.

Reading mode already has smaller/larger text buttons with 14/18/22 px steps.
L36 therefore concerns a preference that survives navigation and restart,
with immediate application to the current reader; adding another pair of
buttons alone would duplicate existing behavior. The settings page already
shows the configured sleep timeout, so L44 requires a real editable bounded
control rather than another readout.

The library currently has bookmark opening/removal and history search/removal.
History searches the full stored list before rendering at most 300 matches.
L23 must make older matching results reachable instead of merely adding a
count to the existing cap. Search all actual source and interactions again
before accepting any candidate; this audit is a starting point.
