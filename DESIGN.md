# K Shell terminal design

## Midnight profile

K Shell's working surface should feel like a blue drafting desk: full paths,
native Windows projects, legible code and a quiet command preview. The distinctive
element is a cyan path above the input, with no frame, clock or repeated brand
label competing with it.

Tokens: Ink blue `#172234` background, Paper `#D6DFEA` text, Glass `#79B9CC`
folders, Sage `#8BCBB2` success, Rose `#E08A90` errors, Slate `#8B99AE` metadata.
Code and archives retain softer lavender/gold accents. Suggestions use a quieter
regular-weight sage/rose/slate version of the status colors. Typed input remains
the host foreground.

Use the installed MesloLGM Nerd Font **Mono**, 13 pt, normal weight. The Mono
variant keeps language/file icons inside the same cell grid as filenames. Place
18 px left/right padding around the text, with 14 px above and 12 px below; use
an opaque background and a thin bar cursor. Keep the existing two-line path/input
layout and vertical listing columns. All terminal controls remain left aligned.

Review: a neon accent and oversized emoji would compete with filenames. Regular
weight cyan paths and one-cell Nerd Font icons keep the hierarchy clear. Avoid
a new banner, animated cursor effects, rounded prompt boxes or background images.
The new palette is opt-in through `theme midnight` or the dedicated Terminal
profile; existing Ocean/Dusk choices retain their behavior.

The terminal is a workspace for navigating Windows projects, reading directory
structures, and entering commands. Its hierarchy should keep the current folder
visible and put the command input on a separate line.

## Suggestion polish plan

Keep typed input in the host's normal foreground. Draw only the unaccepted
suffix as a quiet preview, aligned directly after the cursor. Preserve the
host's monospace font, spacing and background, with no extra badges or labels.

Ocean preview tokens: Sage `#7DB69B` (valid), Dusty rose `#CF8993` (invalid),
Ash `#8C94A2` (unverified), with the existing Frost blue `#81B8EA` path and
Seafoam `#81C7B4` prompt. Dusk previews use `#96BBA4`, `#D094A0`, `#92899E`.
Use regular weight by default; avoid ANSI faint, which varies across hosts.

Offer `theme suggestions soft|bold|underline`: soft is this restrained default,
bold uses the existing stronger status palette, and underline adds a clear
preview cue without adding characters. Accepting a hint returns it to ordinary
input. Both green/red validity and neutral unknown status remain unchanged.

Review: the previous bold hint competed with typed input and the prompt marker.
Reserve stronger emphasis for the user's explicit bold choice. Add no permanent
help text, frames or status icons to the editable command row.

## Icon refresh plan

Use one icon next to the current path and one before each listed name. The
folder icon is the visual anchor; branches, counts and metadata remain quiet.
Keep the existing two-line prompt and full paths, with a red marker alone for
failure. Keep left alignment and measure icons and names together in terminal
cells. Standard Unicode icons are the default; Nerd Font glyphs are opt-in.

Offer two palettes through `theme`: Ocean retains the blue/mint/slate/lavender/
amber/rose tokens below; Dusk uses lavender `#C7AEF2` folders, mint `#A6D7B9`
success, slate `#8F869E` details, pink `#E7A6C5` media, gold `#E4C082` archives
and rose `#F08F93` failure. The host keeps its font and background. Theme and
icon choices are session settings, with optional startup environment defaults.

Review: avoid a decorative dashboard, startup banner or frames around every
line. Put the identity in meaningful file icons instead. Standard icons must
work without installing a font, while a Nerd Font mode serves users who already
have one. Preserve plain, undecorated command output outside the terminal.

## Palette

| Token | Value | Role |
| --- | --- | --- |
| Frost blue | `#81B8EA` | Folder names and current path |
| Seafoam | `#81C7B4` | Input marker, executables, audio, links |
| Slate | `#6F7A8A` | Suggestions, branches, counts, file metadata |
| Lavender | `#C5A6E8` | Images and videos |
| Amber | `#D8B27A` | Archives |
| Rose | `#E28D93` | Input marker after a failed command |

Foreground colors are shared through `presentation::Style`. The terminal host
owns the background, font, size, and line height. Use the host's existing
monospace face; standard Unicode mode requires no extra font installation.
Nerd Font mode is an explicit option for a terminal already using a Nerd Font Mono.

## Layout

Left align paths, input, and output. Use a compact two-line prompt:

```text
📁 D:\My-Portfolio
› tree -L 2
📁 .
├── 📁 app
│   └── ‹› page.tsx
└── ⚙︎ package.json

1 directory, 2 files
```

A failed command changes only the `›` marker to rose red; numeric exit codes
are hidden from the prompt. A successful command restores mint. `ls` uses
three-cell column gaps, measured using visible Unicode widths. Tree branches
and totals stay quiet; folder and file names carry the color.

Typed text retains its normal foreground. The whole suggested suffix uses mint
green for locally validated syntax/previous success and rose red for known
invalid syntax/previous failure. Include suggested flags in the same status:
`do` + suggested `cker --sdf` is entirely red. Unverified arguments stay slate.
For `cl` + suggested `ear`, only the suggested `ear` is green.
The default soft look uses the Sage/Dusty rose/Ash preview tokens above at
regular weight; bold and underline are optional. Underline ends at the last
non-whitespace character so completion padding does not look like extra input.
These colors update on every edit and follow the selected Ocean/Dusk palette.
Accepted hints become normal input. Source text, quotes, spaces and cursor layout
are preserved; suggestion highlighting adds only styling.

## Review against the brief

A framed prompt and repeated K Shell labels were considered and removed: the
tab already names the app, and borders would compete with tree branches. The
two-line prompt is the main layout change. Keep full Windows paths, the user's
chosen background, familiar tree glyphs, and directory names without added `/`.
Avoid banners, animations, mandatory icon fonts, and arbitrary file hiding.

`NO_COLOR` keeps the same hierarchy without escape sequences. Batch and `-c`
output remains plain and prompt-free.
