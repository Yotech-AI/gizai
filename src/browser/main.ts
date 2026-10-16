// GizAi's screens in a plain browser (GA-185, the research's card SG): `npm run browser` serves browser.html, which starts
// here. Tauri's mockIPC stands in for the backend before React starts, answering each command from the fixtures that
// crates/gizai-core/examples/fixtures.rs writes from the demo data. The app's own build has only index.html, so none of
// this is in it.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import fixturesUrl from "./fixtures.json?url";
import { answer, loadFixtures, type Fixtures } from "./mock";

// The blob: URLs alive now (the screenshots' pictures), for the console: gizaiBlobUrls() lists them, so you can see each
// one freed when its picture leaves the screen.
const live = new Set<string>();
const create = URL.createObjectURL.bind(URL), revoke = URL.revokeObjectURL.bind(URL);
URL.createObjectURL = (o: Blob | MediaSource) => { const url = create(o); live.add(url); return url; };
URL.revokeObjectURL = (url: string) => { live.delete(url); revoke(url); };
Object.assign(window, { gizaiBlobUrls: () => [...live] });

const fixtures = (await fetch(fixturesUrl).then((r) => r.json())) as Fixtures;
loadFixtures(fixtures);
mockWindows("main");
mockIPC((cmd, args) => answer(cmd, (args ?? {}) as Record<string, unknown>), { shouldMockEvents: true });
await import("../main");
