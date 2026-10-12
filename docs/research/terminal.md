# Gizai: terminals in the sidebar and a panel in the Code Editor (research, 2026-10-12)

Status: research only, no code. Written by the R&D Agent for GA-124 on 2026-10-12, against `main` at `5bd06a4` (Gizai 0.8.0, with the Editor of GA-91's cards: PR #96 merged that day). `docs/research/code-editor.md` is the model and is cited for what it already established.
Legend: **[V]** = verified today, in the code (file and line), with the command named (`git`, `gh`, `npm view`, `npm ls`, `rg`), or in the primary source named (its link and the date read are in §8). **[I]** = my judgement, or a fact I could not check in a primary source (the reason is given).
Limits: the run may not start node, cargo builds or a browser, so nothing was measured or tried in a webview; what rests on a measurement is marked [I] and given to a card to measure.

(Work in progress: the sections below are being written.)

---

## 3. The feature

### 3.1 What it should do

From the card (Jefsev, 12 Oct 2026):

1. **A Terminal section in the sidebar, above Editor.** The terminals you open are rows under Terminal, as the Editor's folders are rows under Editor. You can close them, so there may be none, 3 or 10: no fixed number.
2. **A click on a terminal's row** opens that terminal in the main panel, to run commands in folders, use git and so on.
3. **A Terminal button in the Code Editor's top bar, between Fetch and Open in.** It opens a terminal panel at the bottom of the Editor, in the root of the folder the Editor shows, like PhpStorm and VS Code. The panel starts at 30% of the Editor's height and can be dragged higher or lower.
4. **The same terminal in both places:** one list of sessions and one terminal component, so a terminal opened in the Editor's panel is also a row under Terminal and can be opened on its own page, and the other way round.

Not in this card: building it, and a terminal for agents.
