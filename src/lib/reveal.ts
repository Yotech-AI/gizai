/** Waits (at most 5 s) until the element with this id is on the page, e.g. in a drawer or a page that is opening, then
 *  scrolls it into view and puts the focus on `focus` (an id) when it can take it, else on the element, or on its first
 *  control when it isn't one itself (GA-185: the empty Screenshots tab's links open a setting this way). */
export function reveal(id: string, focus?: string, until = Date.now() + 5000) {
  const el = document.getElementById(id);
  if (!el) {
    if (Date.now() < until) requestAnimationFrame(() => reveal(id, focus, until));
    return;
  }
  el.scrollIntoView({ block: "center" });
  // a switch that is off and disabled (Take screenshots without the browser) leaves the focus on the first one that isn't
  const wanted = focus ? (document.getElementById(focus) as HTMLInputElement | null) : null;
  const control = (wanted && !wanted.disabled ? wanted : null) || (el.matches("input, select, textarea, button, a[href], [tabindex]")
    ? el : el.querySelector<HTMLElement>("input:not([disabled]), select, textarea, button:not([disabled]), a[href], [tabindex]"));
  (control as HTMLElement | null)?.focus({ preventScroll: true });
}
