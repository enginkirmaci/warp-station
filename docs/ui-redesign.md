# DevTools UI: audit and two directions

Status: audit + one spiked direction. Nothing here is final; the shipping
table is untouched and the prototype only appears with `DEVTOOLS_UI_PROTO=1`.

## How this was produced

The loop used here is the one worth keeping for this app, because GPUI has no
design-tool handoff:

1. **Capture** every surface from the running app (repos list, repo drawer,
   Changes, Overview, Pull Requests empty state, Settings, Summary).
2. **Critique** against a fixed rubric: hierarchy, density, state coverage,
   contrast, affordances, keyboard.
3. **Spike** one direction in code behind an env flag, screenshot it, compare.
4. **Decide**, then wire behaviour and add a headless probe per surface.

Tools that matter for this stack: a coding agent (this harness, Claude Code,
Codex, Cursor) plus screenshots of the real window. Figma, Stitch, v0 and
Lovable are optional for *exploring* layout, but their output is React/HTML
and never lands in GPUI — budget zero time for "export to code".

## Findings, by priority

### P0 — reads as broken

1. **The repo drawer floats over a half-row.** Opening a repo leaves the list
   visible above it, clipped mid-row at the sheet's top edge, with no scrim,
   border or shadow separating the two layers. It looks like a paint bug, not
   a panel.
2. **Rows have no anchor.** Star, name, five launcher icons, branch pill,
   changes chip, GitHub, activity all sit at the same weight; the eye has no
   first stop. The five launchers repeat identically on every row — 30
   identical targets in a six-repo list, all more prominent than the repo name.
3. **The detail header contradicts itself.** `rust` appears twice (neutral
   chip and removable tag), `favorites` and `llm` render as user tags though
   they are system/derived, and each `×` is a ~10px target.
4. **Empty states are voids.** "No open pull requests" is centred in ~500px of
   nothing, duplicating the "0 open pull requests" label directly above it,
   with no action ("Create pull request", "Refresh", "Connect GitHub").

### P1 — friction

5. **Colour carries no stable meaning.** Branch pills are green only for
   `main`, grey otherwise; the header's green `19 issues` chip is a filter but
   reads as a badge; success green means "issues", "staged" and "warp running".
6. **Settings has no reading order.** Two independent columns, toggles and
   text fields at equal weight, and empty fields (`VS Code profile`) that look
   disabled with no explanation.
7. **Toolbar actions are ambiguous.** `Add Repository` is solid primary;
   `Clone` is a bordered ghost with an unrelated cloud icon; refresh is an
   unlabelled lone glyph between them.
8. **Changes view detaches the action from its input.** `Generate & Commit`
   is a large primary button at the bottom of the middle column, visually
   separated from the textarea it acts on; the right rail mixes a branch
   picker with Fetch/Pull/Push/Publish and Sync/History at one weight.
9. **Type does too little work.** 12/13/15px with semibold almost everywhere,
   so hierarchy is carried by colour instead of size and weight.

### P2 — polish

10. The titlebar's `Summary` tab is a transient AI job presented as a place.
11. No visible focus ring in any captured state; the app is mouse-first
    despite `Ctrl+F` being advertised.

## Direction A — "Ledger" (spiked)

One anchor per row, state pushed right, chrome removed.

- Table → **two-line rows**: `name + branch` on the identity line,
  `path · last activity` on the context line.
- Five always-on launchers → **hover-only actions**, favourite star only when
  favourited.
- Group bands (card background + chevron) → **uppercase label + hairline**.
- Changes → **number only, right-aligned**; clean repos say nothing.
- Costs: no sortable headers, no GitHub/PR columns. A real adoption needs a
  sort menu in the toolbar and PR/issue dots on the context line.

Run it: `DEVTOOLS_UI_PROTO=1 cargo run`. Visual only — clicks are not wired.

Best for: the daily "what moved since yesterday" scan.

## Direction B — "Cockpit"

Keep the columns power users scan, but put them on a strict grid and move
detail out of the overlay.

- Fixed grid, 44px single-line rows: star 24 · name 280 · branch 140 ·
  changes 64 · PR 56 · activity 72 · hover actions.
- Group headers become 28px sticky section rows, not card bands.
- Selecting a row opens a **right inspector rail** (300px) instead of a bottom
  sheet, so the list is never covered.
- Numerals right-aligned and tabular; one accent for selection only; amber
  reserved for "dirty"; PR/issue as `· 3` dots rather than columns of dashes.

Best for: triage across 20+ repos, which is where this app is heading.

## Token diff

Both directions want the same substrate change: fewer stacked surfaces, one
accent, quieter hairlines, and a type scale that does the hierarchy work.

| Token | Today | Proposed | Why |
| --- | --- | --- | --- |
| `BACKGROUND` | `#1C1D20` | `#16171A` | Deeper base so rows can sit *on* it without a card |
| `CARD` | `#26272B` | `#1E1F23` | Rows stop reading as stacked cards |
| `BORDER_LIGHT` | `#2A2B30` | `#24252A` | Separators recede; `HAIRLINE` stays for real dividers |
| `TEXT_FAINT` | `#85878F` | `#8A8C94` | 4.16:1 on `CARD` today (fails AA); meta text moves to the page background (4.7:1) |
| `ACCENT` | `#7090CF` | keep | But only for selection, focus and the one primary action |
| new `ROW_HOVER` | — | `rgba(255,255,255,0.04)` | Hover without a full `HOVER` wash |
| new `FOCUS_RING` | — | accent @ 60% | Keyboard-first needs a ring that is not a border colour |
| spacing | ad hoc | 4 / 8 / 12 / 16 / 24 | Row heights and rails stop drifting |
| radius | mixed | 6 (controls) / 8 (panels) | One radius per class |
| type | 12/13/15, semibold everywhere | 11 meta / 13 body / 15 title, weights 400/500/600 | Hierarchy from weight and size, not colour |
| numerals | proportional | tabular | Changes/activity columns stop jittering |

Contrast measured on the current palette: `TEXT_FAINT` on `CARD` is 4.16:1,
below AA for body text — the table's dimmest labels are already borderline.

## Next

1. Pick A or B (or A's rows inside B's grid).
2. Wire the chosen list: selection, hover actions, keyboard nav, sort menu.
3. Rebuild the repo drawer so it never covers the list.
4. Then Settings and Changes, one surface per PR, with a probe each.
