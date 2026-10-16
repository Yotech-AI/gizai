/** Waits (at most 3 s) until the element with this id is on the page, e.g. in a drawer or a page that is opening, then
 *  scrolls it into view and puts the focus on it, or on its first control when it isn't one itself (GA-185: the empty
 *  Screenshots tab's links open a setting this way). */
export function reveal(id: string, until = Date.now() + 3000) {
  const el = document.getElementById(id);
  if (!el) {
    if (Date.now() < until) requestAnimationFrame(() => reveal(id, until));
    return;
  }
  el.scrollIntoView({ block: "center" });
  const control = el.matches("input, select, textarea, button, a[href], [tabindex]")
    ? el : el.querySelector<HTMLElement>("input:not([disabled]), select, textarea, button:not([disabled]), a[href], [tabindex]");
  (control as HTMLElement | null)?.focus({ preventScroll: true });
}
