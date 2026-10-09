// Files picked or dropped before their task exists: only their paths are kept until Create task adds them.

/** A file's type badge: its extension, at most 4 letters, upper case ("FILE" without one). */
export function fileExt(name: string): string {
  return (name.includes(".") ? name.split(".").pop() ?? "file" : "file").slice(0, 4).toUpperCase();
}

/** A path's last part: the file's name. */
export function fileName(path: string): string {
  return path.split("/").filter(Boolean).pop() ?? path;
}

/** The folder a path is in ("/" for one at the root, "" for a bare name). */
export function fileFolder(path: string): string {
  const at = path.replace(/\/+$/, "").lastIndexOf("/");
  return at > 0 ? path.slice(0, at) : at === 0 ? "/" : "";
}

/** `list` and then the paths in `more` it doesn't have yet: a file picked twice is listed once. */
export function addPaths(list: readonly string[], more: readonly string[]): string[] {
  const out = [...list];
  for (const p of more) if (!out.includes(p)) out.push(p);
  return out;
}
