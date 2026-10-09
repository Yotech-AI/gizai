// A click on a gizai: link opens the item's page in Gizai: a task (by its identifier) or a project (by its key) is looked
// up first; a person opens the Users page.
import { itemId } from "../api";
import { go } from "../router";
import type { ItemKind } from "./itemLinks";

export async function openItem(kind: ItemKind, key: string): Promise<void> {
  switch (kind) {
    case "person": go({ page: "users" }); return;
    case "client": go({ page: "client", id: key }); return;
    case "agent": go({ page: "agent", id: key }); return;
    case "doc": go({ page: "doc", id: key }); return;
    // One that is gone opens its page anyway, which says it wasn't found.
    case "task": case "project": go({ page: kind, id: await itemId(kind, key).catch(() => key) }); return;
  }
}
