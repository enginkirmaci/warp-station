# Logo concepts — NOT applied

Four standalone SVG studies for a DevTools mark. Nothing here is wired into the
app (`src/DevTools/logo.ico` and all in-app assets are untouched). Each SVG is
transparent, 512×512, drawn with strokes/paths only (no text, no font deps).

Preview renders live in `preview/` (`-dark-512`, `-dark-64`, `-light-256` per
concept, plus contact sheets).

| File | Idea | Notes |
|---|---|---|
| `c1-stack.svg` | Fanned stack of tool tiles + `</>` | "a suite of tools"; reads as depth/deck |
| `c2-toolbox.svg` | Toolbox silhouette with terminal `>_` prompt | literal toolbox; latch + lid detail |
| `c3-hex.svg` | Hex nut + brackets + green commit node | hardware-tool read; amber/green accents |
| `c4-spark.svg` | `<` sparkle `>` with no container | AI-assisted dev; lightest, most modern |

### c4 variants

Baseline `c4-spark` (green fill spark, blue chevrons) plus nine variants,
side-by-side in `preview/c4-sheet-dark-512.png`:

| File | Change vs baseline | Notes |
|---|---|---|
| `c4-v1-amber.svg` | amber spark | warm accent pops hardest on dark |
| `c4-v2-purple.svg` | purple spark `#A78BFA` | softer, closer to the accent family |
| `c4-v3-gradient.svg` | radial spark, green core → blue tips | glow feel; tips lose a little contrast |
| `c4-v4-double.svg` | smaller main spark + satellite sparkle top-right | classic AI pairing; satellite vanishes at 64 px |
| `c4-v5-outline.svg` | spark stroked, not filled | sketchier/techier; weakest read at 64 px |
| `c4-v6-tile-navy.svg` | mark scaled into a navy rounded tile | app-icon option, dark-first |
| `c4-v7-tile-blue.svg` | blue tile, white chevrons, green spark | strongest app-icon candidate |
| `c4-v8-mono.svg` | everything near-white `#E8ECF5` | titlebar/monochrome use; dark backgrounds only |
| `c4-v9-diamond.svg` | rounded diamond instead of concave spark | "gem" read; calmest of the set |

A six-point spark was tried and dropped: six petals need width the chevron
frame doesn't have, so it read as a cog at every pinch setting.

### v7 tile variants

`c4-v7-tile-blue` (blue tile, white chevrons, green spark) plus eight tile
variants, side-by-side in `preview/v7-sheet-dark-512.png`:

| File | Change vs baseline | Notes |
|---|---|---|
| `c4-v7a-flat.svg` | solid `#3D5BD9`, no gradient | flattest, most uniform read |
| `c4-v7b-electric.svg` | brighter `#5B7CFF→#3350D9` gradient | mark pops most on dark |
| `c4-v7c-diagonal.svg` | top-left → bottom-right gradient | light-source feel, most "designed" |
| `c4-v7d-round.svg` | corner radius 100 | friendly bubble shape |
| `c4-v7e-sharp.svg` | corner radius 40 | harder, more technical |
| `c4-v7f-amber.svg` | amber spark instead of green | striking, but loses the green signature |
| `c4-v7g-ring.svg` | thin inset ring at 16% white | subtle premium badge; invisible at 64 px |
| `c4-v7h-bold.svg` | mark scaled 0.76 (was 0.68) | best small-size legibility |

At 64 px the electric gradient plus the bold scale are the standouts; the ring
only earns its keep at large sizes.

Palette pulled from the app's chip palette: blues `#334EC9`/`#3D5BD9`/`#6E9BF7`,
navy `#26327A`/`#1F2A66`, amber `#FFAB40`, green `#4EC94E`.

All four were checked at 512 px and 64 px on dark (`#17181C`) and light
(`#F4F5F7`) backgrounds. At 64 px the c3 green node is the weakest element; the
c4 sparkle thins slightly but stays legible.
