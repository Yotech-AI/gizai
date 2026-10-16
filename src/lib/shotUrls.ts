// Screenshots' pictures as blob: URLs (GA-185): the bytes come from screenshotBytes, once per picture while something on
// the screen shows it (a thumbnail, the larger view, Compare), and the URL is freed when the last of them goes.
import { useEffect, useState } from "react";
import { screenshotBytes } from "../api";
import { mimeOf } from "./screenshots";

type Entry = { users: number; url: string | null; load: Promise<string> };
const entries = new Map<string, Entry>();

/** The picture's blob: URL; each acquire needs its release. */
export function acquireShot(fileId: string, name: string, load: (id: string) => Promise<ArrayBuffer> = screenshotBytes): Promise<string> {
  let e = entries.get(fileId);
  if (!e) {
    const entry: Entry = { users: 0, url: null, load: Promise.resolve("") };
    entry.load = load(fileId).then((bytes) => {
      const url = URL.createObjectURL(new Blob([bytes], { type: mimeOf(name) }));
      // released while it loaded: freed at once
      if (entries.get(fileId) !== entry) { URL.revokeObjectURL(url); throw new Error("released"); }
      entry.url = url;
      return url;
    });
    entry.load.catch(() => { if (entries.get(fileId) === entry && !entry.url) entries.delete(fileId); });
    entries.set(fileId, entry);
    e = entry;
  }
  e.users++;
  return e.load;
}

export function releaseShot(fileId: string) {
  const e = entries.get(fileId);
  if (!e || --e.users > 0) return;
  entries.delete(fileId);
  if (e.url) URL.revokeObjectURL(e.url);
}

/** How many pictures have a blob: URL now (the self-test and the browser mode's console check it). */
export const shotUrlCount = () => [...entries.values()].filter((e) => e.url).length;

/** A picture's blob: URL while the component shows it (`show`: e.g. once it scrolled into view); freed when it goes. */
export function useShotUrl(shot: { fileId: string; name: string } | null, show = true): { url: string | null; error: string | null } {
  const [state, setState] = useState<{ id: string | null; url: string | null; error: string | null }>({ id: null, url: null, error: null });
  const id = shot?.fileId ?? null, name = shot?.name ?? "";
  useEffect(() => {
    if (!id || !show) return;
    let alive = true;
    acquireShot(id, name).then((url) => { if (alive) setState({ id, url, error: null }); })
      .catch((e) => { if (alive && String(e) !== "Error: released") setState({ id, url: null, error: String(e) }); });
    return () => { alive = false; releaseShot(id); };
  }, [id, name, show]);
  return state.id === id ? { url: state.url, error: state.error } : { url: null, error: null };
}

/** True once the element has come near the screen (it stays true). */
export function useNearScreen<E extends Element>(): [(el: E | null) => void, boolean] {
  const [el, setEl] = useState<E | null>(null);
  const [near, setNear] = useState(false);
  useEffect(() => {
    if (!el || near) return;
    if (typeof IntersectionObserver === "undefined") { setNear(true); return; }
    const io = new IntersectionObserver((es) => { if (es.some((x) => x.isIntersecting)) setNear(true); }, { rootMargin: "300px" });
    io.observe(el);
    return () => io.disconnect();
  }, [el, near]);
  return [setEl, near];
}
