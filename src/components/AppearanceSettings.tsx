// Settings → Appearance (GA-42): the font, the text sizes of the chat, the interface, and tasks and docs, the theme and the
// density. A change shows at once and is kept on this computer (src/lib/appearance.ts), without Save settings.
import { Moon, Sun } from "lucide-react";
import { Field, FormSection } from "./Form";
import {
  CHAT_SIZES, DEFAULTS, DOCS_SIZES, FONTS, UI_SIZES, isDefault, resetAppearance, setAppearance, useAppearance,
} from "../lib/appearance";

/** One size picker: the sizes in px, the default marked in its title. */
function Sizes({ label, sizes, value, base, onPick }: { label: string; sizes: number[]; value: number; base: number; onPick: (px: number) => void }) {
  return (
    <div className="seg size-seg" role="group" aria-label={label}>
      {sizes.map((n) => (
        <button key={n} aria-pressed={value === n} title={n === base ? `${n} px, the default` : `${n} px`} onClick={() => onPick(n)}>{n}</button>
      ))}
    </div>
  );
}

export function AppearanceSettings() {
  const a = useAppearance();
  const atDefaults = isDefault(a);
  return (
    <>
      <div className="appearance-note">
        <span className="faint">Changes show at once, without Save settings, and are kept on this computer.</span>
        <button className="link" onClick={resetAppearance} disabled={atDefaults} title={atDefaults ? "Everything is at its default" : undefined}>Reset to defaults</button>
      </div>
      <FormSection title="Font" text="One font for the whole app: each choice sets the text font and the code font (task IDs, code and run logs). Claude's own fonts are licensed, so Gizai can't ship them; Inter and Geist give a similar clean look.">
        <Field label="Font" wide>
          <div className="font-choices" role="radiogroup" aria-label="Font">
            {FONTS.map((f) => (
              <label key={f.key} className={`font-choice${a.font === f.key ? " on" : ""}`} data-font={f.key}>
                <input type="radio" name="gizai-font" checked={a.font === f.key} onChange={() => setAppearance({ font: f.key })} />
                <span className="fc-name">{f.name}</span>
                <span className="fc-sample">Export invoices as CSV <span className="mono">KADE-41</span></span>
                <span className="fc-note">{f.note}</span>
              </label>
            ))}
          </div>
        </Field>
      </FormSection>
      <FormSection title="Text size" text="In px of the main text. Headings grow about half as much; IDs, labels, times, badges and icons stay about the same.">
        <Field label="Chat size" wide hint="Your messages, the agent's messages and the composer. The chat column gets wider with it, so lines stay about as long.">
          <Sizes label="Chat size" sizes={CHAT_SIZES} value={a.chat} base={DEFAULTS.chat} onPick={(chat) => setAppearance({ chat })} />
        </Field>
        <Field label="Interface size" wide hint="The sidebar, top bar, lists, board, forms, menus and the command palette. Rows, the sidebar and board columns grow with it.">
          <Sizes label="Interface size" sizes={UI_SIZES} value={a.ui} base={DEFAULTS.ui} onPick={(ui) => setAppearance({ ui })} />
        </Field>
        <Field label="Tasks and docs size" wide hint="Descriptions, acceptance criteria, comments and docs, when you read them and when you edit them.">
          <Sizes label="Tasks and docs size" sizes={DOCS_SIZES} value={a.docs} base={DEFAULTS.docs} onPick={(docs) => setAppearance({ docs })} />
        </Field>
        <Field label="Preview" wide>
          <div className="size-preview">
            <div className="panel chat-sample" aria-label="Chat">
              <span className="faint">Chat</span>
              <div className="chat-msg user"><div className="bubble">Can you split the export into two cards?</div></div>
              <div className="chat-msg agent"><div className="prose"><p>Yes: <b>KADE-12</b> exports the invoices and <b>KADE-13</b> mails them each month.</p></div></div>
            </div>
            <div className="panel docs-sample" aria-label="Tasks and docs">
              <span className="faint">Tasks and docs</span>
              <div className="prose"><h3>Acceptance criteria</h3><p>The portal replaces the Excel lists Kade's office keeps today.</p></div>
            </div>
          </div>
        </Field>
      </FormSection>
      <FormSection title="Theme and density" text="Also with the t and d keys, when you aren't typing.">
        <Field label="Theme">
          <div className="seg" role="group" aria-label="Theme">
            <button aria-pressed={a.theme === "dark"} onClick={() => setAppearance({ theme: "dark" })}><span className="seg-label"><Moon className="icon sm" />Dark</span></button>
            <button aria-pressed={a.theme === "light"} onClick={() => setAppearance({ theme: "light" })}><span className="seg-label"><Sun className="icon sm" />Light</span></button>
          </div>
        </Field>
        <Field label="Density" hint="Compact makes rows, menus, buttons and fields shorter.">
          <div className="seg" role="group" aria-label="Density">
            <button aria-pressed={a.density === "comfortable"} onClick={() => setAppearance({ density: "comfortable" })}>Comfortable</button>
            <button aria-pressed={a.density === "compact"} onClick={() => setAppearance({ density: "compact" })}>Compact</button>
          </div>
        </Field>
      </FormSection>
    </>
  );
}
