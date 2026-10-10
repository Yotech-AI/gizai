// The Memory page's graph (GA-69): the global graph (Notes | Graph → Graph) of the page's notes, and the local graph
// beside an open note (its neighbours, depth 1 to 5, incoming and outgoing), which follows the note that is open. Both
// draw on GraphCanvas, have the settings panel (GraphPanel) and list a dot's connections for the keyboard: the local
// graph its note's, the global graph the dot chosen with the arrow keys.
import { useMemo, useRef, useState } from "react";
import { ArrowLeft, ArrowLeftRight, ArrowRight, Maximize2, Minus, Plus, Settings2, X } from "lucide-react";
import { getTeam, listClients, listProjects, listTasks, listUsers } from "../../api";
import { useData } from "../../lib/useData";
import { useCurrentTeam } from "../../lib/team";
import {
  buildGraph, connections, defaultGroups, groupOf, graphView, KIND_NAME, parseGraphQuery, type Connection, type Graph, type GraphGroup, type GraphNode,
  type GraphSettings, type PaletteKey,
} from "../../lib/graph";
import type { MemoryNote } from "../../types";
import { GraphCanvas, type GraphHandle } from "./GraphCanvas";
import { GraphPanel, KindIcon } from "./GraphPanel";

/** What notes can name, for the graph's other kinds of dot: cards, projects, clients, agents and people. */
function useGraphSources() {
  const [teamId] = useCurrentTeam();
  const cards = useData(() => listTasks({ openOnly: false }).catch(() => []));
  const projects = useData(() => listProjects().catch(() => []));
  const clients = useData(() => listClients().catch(() => []));
  const people = useData(() => listUsers().catch(() => []));
  const team = useData(() => getTeam(teamId).catch(() => null), [teamId]);
  return {
    cards: cards.data ?? [], projects: projects.data ?? [], clients: clients.data ?? [], people: people.data ?? [],
    agents: (team.data?.members ?? []).filter((m) => m.kind === "agent"),
  };
}

/** The whole graph of every note (links resolve among all of them), and the notes by id. */
function useFullGraph(notes: readonly MemoryNote[]) {
  const src = useGraphSources();
  // Built again only when what it is made of changed, not each time the notes are read again after a write elsewhere.
  const key = [
    notes.map((n) => `${n.id}:${n.currentVersion}:${n.path}`).join("|"),
    src.cards.map((c) => `${c.identifier}:${c.title}`).join("|"), src.projects.map((p) => `${p.id}:${p.key}:${p.name}`).join("|"),
    src.clients.map((c) => `${c.id}:${c.name}`).join("|"), src.people.map((p) => `${p.id}:${p.handle}:${p.name}`).join("|"),
    src.agents.map((a) => `${a.actorId}:${a.handle}:${a.name}`).join("|"),
  ].join("\n");
  const graph = useMemo(() => buildGraph({ notes, ...src }), [key]); // eslint-disable-line react-hooks/exhaustive-deps
  const byId = useMemo(() => new Map(notes.map((n) => [n.id, n])), [notes]);
  return { graph, byId };
}

/** Each note dot's group colour: the first group whose query the note matches. */
function useGroupColors(view: Graph, byId: ReadonlyMap<string, MemoryNote>, groups: GraphGroup[]) {
  return useMemo(() => {
    const parsed = groups.map((g) => ({ query: parseGraphQuery(g.query), color: g.color }));
    const out = new Map<string, PaletteKey>();
    for (const n of view.nodes) {
      const note = n.kind === "note" ? byId.get(n.ref) : undefined;
      const c = note ? groupOf(note, parsed) : null;
      if (c) out.set(n.id, c);
    }
    return out;
  }, [view, byId, groups]);
}

type GraphProps = {
  /** Every note, with its text. */
  notes: MemoryNote[];
  /** Whether a note is one of the page's (an agent's page: its folder). */
  inScope: (n: MemoryNote) => boolean;
  settings: GraphSettings;
  onSettings: (s: GraphSettings) => void;
  onReset: () => void;
  onOpen: (n: GraphNode) => void;
};

const count = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** The global graph: every note of the page, what they link to and name. */
export function GlobalGraph({ notes, inScope, settings, onSettings, onReset, onOpen, focus }: GraphProps & { focus?: string | null }) {
  const { graph, byId } = useFullGraph(notes);
  const scoped = useMemo(() => notes.filter(inScope), [notes, inScope]);
  const scope = useMemo(() => { const ids = new Set(scoped.map((n) => n.id)); return (id: string) => ids.has(id); }, [scoped]);
  const view = useMemo(() => graphView(graph, settings, byId, { scope }), [graph, settings, byId, scope]);
  const groups = settings.groups ?? defaultGroups(scoped);
  const colors = useGroupColors(view, byId, groups);
  const panel = settings.panel;
  const setPanel = (on: boolean) => onSettings({ ...settings, panel: on });
  const [chosen, setChosen] = useState<string | null>(null);
  const canvas = useRef<GraphHandle>(null);
  const notesShown = view.nodes.filter((n) => n.kind === "note").length;
  const chosenNode = chosen ? view.nodes.find((n) => n.id === chosen) ?? null : null;
  const focusId = focus ? `note:${focus}` : null;
  return (
    <div className="graph-wrap" aria-label="Graph of the notes">
      <GraphCanvas graph={view} settings={settings} colors={colors} focus={focusId && view.nodes.some((n) => n.id === focusId) ? focusId : null}
        selected={chosenNode?.id ?? null} onSelect={setChosen} onOpen={onOpen} handle={canvas}
        label={`Graph of ${count(notesShown, "note", "notes")} and ${count(view.links.length, "link", "links")}. The arrow keys go from dot to dot and list its connections, Enter opens it.`} />
      <div className="graph-tools">
        <span className="faint graph-count">{count(notesShown, "note", "notes")} · {count(view.links.length, "link", "links")}</span>
        <ZoomTools canvas={canvas} />
      </div>
      {!panel && (
        <button className="btn sm graph-panel-open" aria-label="Graph settings" title="Filters, groups, display and forces" onClick={() => setPanel(true)}>
          <Settings2 className="icon" />Settings
        </button>
      )}
      {panel && <GraphPanel settings={settings} onChange={onSettings} groups={groups} onAnimate={() => canvas.current?.animate()} onReset={onReset} onClose={() => setPanel(false)} />}
      {graph.nodes.length === 0 ? <p className="graph-empty faint">No notes yet: the graph shows them and their links once there are some.</p>
        : view.nodes.length === 0 ? <p className="graph-empty faint">No dot matches these filters.</p> : null}
      {chosenNode && (
        <div className="graph-connections" role="region" aria-label={`Connections of ${chosenNode.label}`}>
          <div className="graph-connections-head">
            <KindIcon kind={chosenNode.kind} /><button className="link-btn ellipsis" onClick={() => onOpen(chosenNode)}>{chosenNode.label}</button>
            <button className="btn ghost sm icon-only" aria-label="Close the connections" title="Close" onClick={() => setChosen(null)}><X className="icon" /></button>
          </div>
          <Connections list={connections(view, chosenNode.id)} onOpen={onOpen} />
        </div>
      )}
    </div>
  );
}

/** The local graph beside the open note: its neighbours, as deep and in the directions the settings say. */
export function LocalGraph({ notes, inScope, settings, onSettings, onReset, onOpen, noteId, onClose }: GraphProps & { noteId: string; onClose: () => void }) {
  const { graph, byId } = useFullGraph(notes);
  const centre = `note:${noteId}`;
  const scoped = useMemo(() => notes.filter(inScope), [notes, inScope]);
  const view = useMemo(() => graphView(graph, settings, byId, { centre }), [graph, settings, byId, centre]);
  const groups = settings.groups ?? defaultGroups(scoped);
  const colors = useGroupColors(view, byId, groups);
  const panel = settings.panel;
  const setPanel = (on: boolean) => onSettings({ ...settings, panel: on });
  const [chosen, setChosen] = useState<string | null>(null);
  const canvas = useRef<GraphHandle>(null);
  const note = byId.get(noteId);
  const list = connections(view, centre);
  return (
    <aside className="mem-local" aria-label="Local graph">
      <div className="mem-local-head">
        <b>Local graph</b>
        <span className="faint ellipsis" title={note?.path}>{note ? note.path.slice(note.path.lastIndexOf("/") + 1) : ""}</span>
        <button className={`btn ghost sm icon-only${panel ? " on" : ""}`} aria-pressed={panel} aria-label="Local graph settings" title="Depth, links, filters, groups, display and forces" onClick={() => setPanel(!panel)}>
          <Settings2 className="icon" />
        </button>
        <button className="btn ghost sm icon-only" aria-label="Close the local graph" title="Close the local graph" onClick={onClose}><X className="icon" /></button>
      </div>
      <div className="graph-wrap local">
        <GraphCanvas graph={view} settings={settings} colors={colors} focus={centre} centre={centre} selected={chosen && view.nodes.some((n) => n.id === chosen) ? chosen : null}
          onSelect={setChosen} onOpen={onOpen} handle={canvas} maxFit={2.4}
          label={`Local graph of ${note ? note.path : "the note"}: ${count(view.nodes.length - 1, "dot", "dots")} around it. The arrow keys go from dot to dot, Enter opens it.`} />
        <div className="graph-tools"><ZoomTools canvas={canvas} /></div>
        {panel && <GraphPanel local settings={settings} onChange={onSettings} groups={groups} onAnimate={() => canvas.current?.animate()} onReset={onReset} onClose={() => setPanel(false)} />}
      </div>
      <div className="mem-local-list">
        <div className="mem-sub">Connections <span className="faint">{list.length}</span></div>
        {list.length === 0 ? <p className="faint">{settings.incoming || settings.outgoing ? "Nothing links here, and this note links to nothing the filters show." : "Incoming and outgoing links are both off."}</p>
          : <Connections list={list} onOpen={onOpen} />}
      </div>
    </aside>
  );
}

/** Fit, zoom in and zoom out: the canvas's buttons, for those who don't use the wheel. */
function ZoomTools({ canvas }: { canvas: React.RefObject<GraphHandle | null> }) {
  return (
    <span className="graph-zoom">
      <button className="btn ghost sm icon-only" aria-label="Zoom in" title="Zoom in (+)" onClick={() => canvas.current?.zoom(1.3)}><Plus className="icon" /></button>
      <button className="btn ghost sm icon-only" aria-label="Zoom out" title="Zoom out (-)" onClick={() => canvas.current?.zoom(1 / 1.3)}><Minus className="icon" /></button>
      <button className="btn ghost sm icon-only" aria-label="Fit the graph in view" title="Fit the graph in view (0)" onClick={() => canvas.current?.fit()}><Maximize2 className="icon" /></button>
    </span>
  );
}

const WAY = {
  out: { Icon: ArrowRight, say: "links to" },
  in: { Icon: ArrowLeft, say: "linked from" },
  both: { Icon: ArrowLeftRight, say: "both ways" },
} as const;

/** A dot's connections as buttons: each opens that dot (a note, a card, a project…); Tab reaches them. */
function Connections({ list, onOpen }: { list: Connection[]; onOpen: (n: GraphNode) => void }) {
  return (
    <ul className="graph-conn-list">
      {list.map((c) => {
        const { Icon, say } = WAY[c.way];
        return (
          <li key={c.node.id}>
            <button className="mem-row" title={c.node.path ?? c.node.detail ?? c.node.label} onClick={() => onOpen(c.node)}>
              <Icon className="icon sm" /><span className="sr-only">{say}</span>
              <KindIcon kind={c.node.kind} />
              <span className={`ellipsis${c.node.kind === "missing" ? " graph-missing" : ""}`}>{c.node.label}</span>
              <span className="faint ellipsis">{c.node.kind === "note" ? c.node.detail : KIND_NAME[c.node.kind]}</span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}
