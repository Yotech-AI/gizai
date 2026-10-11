// GA-104, Code Editor D1: what makes changes.html move. Dark, light or both (the t key switches, as in the app); frames
// scale to the window; and inside the Changes frames the carets, folds, Show buttons, the file list, the filter and the
// keys (n, p, [, Alt+↓, Alt+↑) work the way F1 should make them work.
(() => {
  const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];

  // Dark, light or both: every themed block gets a light twin; the mode says which show.
  for (const block of $$(".themed")) {
    const twin = block.cloneNode(true);
    twin.dataset.theme = "light";
    twin.classList.add("twin");
    for (const el of $$(".theme-name", twin)) el.textContent = "Light";
    for (const el of $$("[id]", twin)) el.removeAttribute("id");
    block.after(twin);
  }
  const setMode = (mode) => {
    document.body.dataset.mode = mode;
    document.documentElement.dataset.theme = mode === "light" ? "light" : "dark";
    for (const b of $$(".dp-mode button")) b.setAttribute("aria-pressed", String(b.dataset.mode === mode));
    try { localStorage.setItem("gizai-d1-mode", mode); } catch { /* opened from disk without storage */ }
    fit();
  };
  document.addEventListener("click", (e) => {
    const b = e.target.closest(".dp-mode button");
    if (b) setMode(b.dataset.mode);
  });

  // Frames are drawn at the app's size (1440 wide) and scaled down to fit a narrower window.
  function fit() {
    for (const wrap of $$(".frame-fit")) {
      const frame = wrap.firstElementChild;
      const w = parseFloat(frame.style.width) + 2;
      const zoom = Math.min(1, wrap.clientWidth / w);
      frame.style.zoom = zoom < 1 ? zoom.toFixed(3) : "";
    }
  }
  addEventListener("resize", fit);

  // ---- the Changes view ----
  const boxOf = (el) => el.closest(".chg-box");
  const scroller = (el) => el.closest(".chg")?.querySelector(".chg-diffs");
  function current(chg, file) {
    for (const r of $$(".chg-file", chg)) r.classList.toggle("on", r === file);
    file?.scrollIntoView({ block: "nearest" });
  }
  function goTo(row) {
    const chg = row.closest(".chg");
    const box = $$(".chg-box", chg).find((b) => b.dataset.path === row.title.split(" → ").pop());
    if (!box) return;
    current(chg, row);
    const diffs = scroller(row);
    diffs.scrollTo({ top: box.offsetTop - diffs.offsetTop - 8, behavior: "smooth" });
  }
  // A fold's hidden rows: all of them, or 20 from the change above (down) or the change below (up).
  function unfold(bar, step) {
    const hidden = bar.nextElementSibling;
    const rows = [...hidden.children];
    const take = step ? Math.min(20, rows.length) : rows.length;
    if (step === "down") bar.before(...rows.slice(0, take));
    else hidden.after(...rows.slice(rows.length - take));
    const left = hidden.children.length;
    if (!left) { bar.remove(); hidden.remove(); return; }
    bar.querySelector(".fold-all").textContent = left + " unchanged lines";
    bar.querySelector(".fold-all").title = "Show all " + left + " lines";
    if (left <= 20) for (const s of $$(".fold-step", bar)) s.remove();
  }
  document.addEventListener("click", (e) => {
    const el = e.target.closest("button, a");
    if (!el || !el.closest(".frame, .spec")) return;
    if (el.matches(".chg-file")) { e.preventDefault(); goTo(el); return; }
    if (el.matches(".chg-caret")) {
      const box = boxOf(el), folded = !box.classList.contains("folded");
      box.classList.toggle("folded", folded);
      el.setAttribute("aria-expanded", String(!folded));
      el.title = (folded ? "Show" : "Fold") + " this file";
      return;
    }
    if (el.matches("[data-show]")) {
      const box = boxOf(el);
      box.classList.remove("folded");
      box.querySelector(".chg-caret")?.setAttribute("aria-expanded", "true");
      for (const later of $$(".chg-later", box)) later.hidden = false;
      return;
    }
    if (el.matches(".fold-all")) { unfold(el.closest(".fold")); return; }
    if (el.matches(".fold-step")) { unfold(el.closest(".fold"), el.dataset.step); return; }
    if (el.matches(".chg-dir")) {
      const open = el.getAttribute("aria-expanded") !== "true";
      el.setAttribute("aria-expanded", String(open));
      el.querySelector("use")?.setAttribute("href", open ? "#i-chevron-down" : "#i-chevron-right");
      return;
    }
    if (el.matches("[data-foldlist]")) { el.closest(".chg").classList.add("list-folded"); return; }
    if (el.matches("[data-clear]")) {
      const input = el.closest(".chg-files").querySelector(".chg-filter input");
      input.value = "";
      filter(input);
    }
  });

  // The filter narrows the list and the diffs to the paths that hold its text.
  function filter(input) {
    const chg = input.closest(".chg"), q = input.value.trim().toLowerCase();
    let shown = 0;
    for (const row of $$(".chg-file", chg)) {
      const path = row.title.split(" → ").pop();
      const hit = !q || row.title.toLowerCase().includes(q);
      row.hidden = !hit;
      shown += hit ? 1 : 0;
      const box = $$(".chg-box", chg).find((b) => b.dataset.path === path);
      if (box) box.hidden = !hit;
    }
    for (const dir of $$(".chg-dir", chg)) dir.hidden = !$$(".chg-file", dir.nextElementSibling).some((r) => !r.hidden);
    const none = chg.querySelector(".chg-nomatch");
    none.hidden = shown > 0;
    none.querySelector("b").textContent = "“" + input.value.trim() + "”";
  }
  document.addEventListener("input", (e) => { if (e.target.closest(".chg-filter")) filter(e.target); });

  // While a Changes frame has the focus: n and p for files, [ for the list, Alt+↓ and Alt+↑ for the changes in a file.
  document.addEventListener("keydown", (e) => {
    if (e.target.closest("input, textarea") || e.ctrlKey || e.metaKey) return;
    if (e.key === "t" && !e.altKey) { setMode(document.body.dataset.mode === "dark" ? "light" : "dark"); return; }
    const chg = (document.activeElement && document.activeElement.closest(".chg")) || null;
    if (!chg) return;
    const rows = $$(".chg-file", chg).filter((r) => !r.hidden);
    const at = rows.findIndex((r) => r.classList.contains("on"));
    if (!e.altKey && (e.key === "n" || e.key === "p")) {
      e.preventDefault();
      const next = rows[Math.max(0, Math.min(rows.length - 1, at + (e.key === "n" ? 1 : -1)))];
      if (next) goTo(next);
    } else if (!e.altKey && e.key === "[") {
      e.preventDefault();
      chg.classList.toggle("list-folded");
    } else if (e.altKey && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      e.preventDefault();
      const diffs = chg.querySelector(".chg-diffs");
      const top = diffs.getBoundingClientRect().top + 48;
      const starts = $$(".r.add, .r.del", diffs).filter((r) => !r.previousElementSibling?.matches(".r.add, .r.del") && r.offsetParent);
      const y = (r) => r.getBoundingClientRect().top - top;
      const target = e.key === "ArrowDown" ? starts.find((r) => y(r) > 2) : starts.reverse().find((r) => y(r) < -2);
      if (target) diffs.scrollBy({ top: y(target), behavior: "smooth" });
    }
  });
  // The current file follows the scroll: the first file whose box reaches under the sticky header.
  document.addEventListener("scroll", (e) => {
    const diffs = e.target;
    if (!(diffs instanceof Element) || !diffs.matches(".chg-diffs")) return;
    const top = diffs.getBoundingClientRect().top + 40;
    const box = $$(".chg-box", diffs).find((b) => !b.hidden && b.getBoundingClientRect().bottom > top);
    const chg = diffs.closest(".chg");
    if (box) current(chg, $$(".chg-file", chg).find((r) => r.title.split(" → ").pop() === box.dataset.path));
  }, true);
  // A click in a Changes frame gives it the keys.
  document.addEventListener("pointerdown", (e) => { const chg = e.target.closest(".chg"); if (chg && !e.target.closest("input, button, a")) chg.focus({ preventScroll: true }); });

  let mode = "dark";
  try { mode = localStorage.getItem("gizai-d1-mode") || "dark"; } catch { /* no storage */ }
  setMode(mode);
})();
