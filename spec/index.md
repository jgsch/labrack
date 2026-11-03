# Spec index

LAB.RACK: a desktop app to control the DSP of Lab.gruppen amplifiers (IPD and IPX) over IP,
replacing the official app.

This spec describes the agreed behavior of the product. A project starts by
writing a small spec; changes edit it directly, and git keeps its history.
Plans live in `plan/roadmap.md` and `plan/backlog.md`. Read this index, then
only the files your task touches.

| File               | Prefix | Covers                                                  | Read when                                |
|--------------------|--------|---------------------------------------------------------|------------------------------------------|
| `behavior.md`      | `BEH`  | Hardware model, amp list, discovery, mute, link, presets | Almost always                            |
| `ux.md`            | `UX`   | Navigation, visual style, pages, interaction details     | Changing anything user-facing            |
| `architecture.md`  | `ARCH` | Crates, data flow, testing, development environment      | Adding modules, storage, tasks or tools  |
| `security.md`      | `SEC`  | Safety rules for writing to amps, network assumptions    | Touching anything that writes to an amp  |
| `performance.md`   | `PERF` | Platforms, meter and startup budgets, offline use        | Changing hot paths or dependencies       |
| `mockup.html`      |        | UI reference boards (`mise run mockup`)                  | Changing anything user-facing            |
| `mockup/`          |        | The same mockup as plain files, fonts and licences       | Looking at a board's markup or CSS       |

`mockup/` is the source and `mockup.html` the single-file form built from it (`mise run
mockup-pack`). Edit the board templates in `mockup/boards/`, then pack. `mise run mockup-unpack`
goes the other way for a new export of the mockup. `scripts/mockup.py` describes the layout.

## Conventions

- Each requirement is a testable statement with an ID made of its file prefix
  and a number, e.g. `BEH-4`. Tests and change files reference these IDs.
- IDs are never reused; a removed requirement keeps its number retired.
- When a file passes ~300 lines, split it and update this index.
