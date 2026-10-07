import { useEffect, useState } from "react";
import { listClis } from "../api";
import type { CliStatus } from "../types";

/** The coding CLIs, asked once when the screen opens: finding their programs asks your login shell, so not on every change. */
export function useClis(): CliStatus[] | null {
  const [clis, setClis] = useState<CliStatus[] | null>(null);
  useEffect(() => {
    let alive = true;
    listClis().then((c) => alive && setClis(c)).catch(() => alive && setClis([]));
    return () => { alive = false; };
  }, []);
  return clis;
}
