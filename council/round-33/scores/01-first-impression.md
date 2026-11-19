# first impression

score: 6.4/10

## What I looked at

`reference/reference-window.png` (1500x917) and `reference/reference-1740.png` (1740x1011) against
`council/round-33/01-default.png`, plus `02-pins.png`, `03-tabs.png`, `08-empty.png`, `09-missing.png`
and the dark pair, all measured with ImageMagick crops (`-crop WxH+X+Y … txt:-`, `-format "%[pixel:p{x,y}]"`).
Source read for attribution: `src/ui.rs`, `src/css.rs`, `src/app.rs`, `tools/demo/office.html`,
`docs/DIFFERENCES.md`.

**Limitation, stated plainly:** the visual provider I was asked to use for image assessment was
unavailable for this round, so this review is eyeball-plus-instrumented-pixels rather than
model-assisted visual scoring. Every number below is a real measurement from the committed PNGs, but
I could not run the requested semantic image pass; a re-score with that provider available is
worth doing before the round is called.

## What is right

- **The silhouette is the reference's.** Same 1500x917 frame, left rail ending at x=231, wash band
  28-29px tall (REF x=200 y110-138, build y98-125), and the felted grey row text lands on the same
  value band (idle rows REF 128-132, build 106-113 on `#6A6A68` from `src/css.rs:33`).
- **The footer menu mark is faithful, not invented.** Build draws it at (19,891)-(28,898); the
  larger reference (`reference/reference-1740.png`) carries the same glyph at (84,877)-(115,916).
  The 1500px ref crop simply loses it.
- **Chrome-vs-page separation holds in dark** (`council/round-33-dark/01-default.png`): rail 11,11,11,
  divider x=231 at 60, page still white — the fixture page is a light document, which is correct.
- **Nothing jumps or reflows between states** — 01, 02, 03, 07, 08 and 09 all keep the rail width,
  control cluster position and content column origin identical, which is what makes a browser feel
  finished before any individual pixel is right.

## What costs points

**1. The live tab row is bold; the reference never bolds a row.** (weight, most visible)
`src/ui.rs:602` adds class `liveword` when the row is live, and `src/css.rs:223` styles it
`font-weight: bold; color: {liveink}`. Measured: build live row glyphs sit at gray 14 with an ink
ratio of 0.235 over 110x11 px; reference's live row is *regular* weight at gray 7-14 with 0.366 ink
over 92x10. So the build trades the reference's quiet near-black for a heavier, wider, shoutier row —
in `01-default` "New tab" and in `03-tabs` "Reading" (562 dark pixels in one row band, y196-206) the
row pulls the eye off the content entirely. **Smallest fix:** drop `font-weight: bold` from
`.liveword` at `src/css.rs:223`; keep only the colour change to `liveink`. **Cost:** one line, and
the sidebar immediately reads like the reference.

**2. A 1px white seam runs around the whole window, and the titlebar has no gradient.**
Reference at x=0: 240 at y=0, 239 y=7-12, 238 y=13-18, 237 y=19-23, 235 y=29-30, white by y≈33.
Build at x=0: `#FFFFFF` at y=0, then a flat `#EAEAEA` (234) from y=1 to y=30 with no gradient at all —
and x=1499 is white for 894 of 917 rows. Read at 100% it looks like a printing misregistration: a
bright hairline glued to the outside of a dead-flat band. **Smallest fix:** in the window/`headerbar`
CSS add `border-top: 1px solid` using the existing faint line colour, and replace the flat band
background with a 2-stop linear-gradient (240 → 235 over the first 24px). **Cost:** two CSS
declarations; the top 30 rows stop looking stuck-on.

**3. The demo page in `01-default` sits 13-21px too low and wraps one link onto its own line.**
Reference h1 ink (572,378)-(691,390), body bands y422-430 / 446-457 / 474-483 / 515-523, button
(572,579)-(700,617) at 128x39. Build h1 (575,389)-(699,402), body y435-443 / 461-469 / 486-494 /
524-534, button (574,600)-(704,640) at 130x41. That is ~11px on the heading, ~13px on the body and
21px on the button, plus "LinkedIn" wrapping to a line of its own because `h1 { font-size: 17px }`
(`tools/demo/office.html:17`) is a point larger than the reference's 16px and `main { width: 585px }`
buys a slightly narrow measure. **Smallest fix:** `main { width: 596px; transform: translateY(28px) }`
and `h1 { font-size: 16px }` in `tools/demo/office.html`. **Cost:** two numbers; the default state's
whole centre of gravity moves back onto the reference's optical centre.

**4. Links have no underline, so the page has no link texture.** `tools/demo/office.html:22` sets
`a { color: #6B6B6B; text-decoration: none; }`. Reference underlines every inline link — measured run
at y=514 under "Public", and on "hello@officecommun.com", "Instagram", "X", "LinkedIn". In the build
the only cue is bold. **Smallest fix:** `text-decoration: underline; text-decoration-thickness: 1px;
text-underline-offset: 2px`. **Cost:** one declaration; the paragraph regains its reference rhythm.

**5. Letter tiles stand in for site marks, and they are too heavy.** `src/ui.rs`/`src/app.rs` draw
`tab.letter()` when the fixture page supplies no icon, so `01-default` shows B/O/X/F/R capitals at
fill 239 / letter 106, 24x24. Reference tiles are ~32px wide with fill ~226-245, letter ~195, and the
live tab wears the Office Commun ⌘-like mark rather than a letter. **Smallest fix:** lighten the
no-icon fallback (letter `#B4B4B4`, fill `#F0F0F0`) and give `tools/demo/office.html` a `link
rel="icon"` data-URI so the live row shows a mark. **Cost:** small CSS value change plus a fixture
line.

**6. `02-pins` reads as a sliver, not four pins.** In `02-pins.png` the 222-246 grey band over
x=8..207 spans only y20-23 with 800 pixels — the four tiles are clipped to a 4px strip where the
reference's strip occupies y40-98. **Smallest fix:** add the missing vertical padding/bottom inset on
the pin strip container so each tile gets its full ~22px height. **Cost:** one padding value; the
state stops looking broken at first glance, which is exactly what a first-impression pass catches.

**7. "+ New tab" is as dark as an idle row.** Build min gray 106 (`FAINT = hex(0x6A6A68)` at
`src/css.rs:41`) against the reference's 132 — the reference makes that row the faintest thing in the
column. **Smallest fix:** point `FAINT`'s light value at `0x8A8A88`. **Cost:** one constant; the rail
gains its intended depth order (live > idle > action).

## The three changes that would raise this score most

1. **Stop bolding the live row** — remove `font-weight: bold` from `.liveword` (`src/css.rs:223`).
   It is the loudest type error in every state and the reference is emphatically regular-weight.
2. **Kill the 1px white outline and gradient the titlebar band** — `border-top: 1px solid` + a
   240→235 gradient over the first 24px. The window currently has a bright hairline on all four
   sides of a flat plate; fixing it is what separates "app" from "screenshot of an app".
3. **Re-seat the default page on the reference's optical centre** — `main { width: 596px; transform:
   translateY(28px) }`, `h1 { font-size: 16px }`, links underlined in `tools/demo/office.html`.
   A first impression is mostly the first state's centre of gravity, and today it is 13-21px low with
   a wrapped link.
