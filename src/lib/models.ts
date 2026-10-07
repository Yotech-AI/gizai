// The model and effort pickers in the agent form: Claude Code's own list (claude_models), matched by alias or
// full id, so a name like "opus 5.5" that Claude Code doesn't know is caught before a run fails on it.
import type { ModelOption } from "../types";

/** Claude Code's effort levels, lowest first. */
export const EFFORTS = ["low", "medium", "high", "xhigh", "max"];

/** The offered model a saved value means; empty means Claude Code's default. */
export function findModel(value: string | null | undefined, models: ModelOption[] | null): ModelOption | undefined {
  const v = (value ?? "").trim().toLowerCase() || "default";
  return (models ?? []).find((m) => m.value.toLowerCase() === v || (m.resolvedModel ?? "").toLowerCase() === v);
}

/** The effort levels the model takes (all of them when we can't tell). */
export function effortChoices(value: string | null | undefined, models: ModelOption[] | null): string[] {
  const m = models ? findModel(value, models) : undefined;
  return m ? m.effortLevels : EFFORTS;
}

/** One line under the picker: the model's description, or why a saved name won't work. */
export function modelHint(value: string | null | undefined, models: ModelOption[] | null): string {
  if (!models) return "Empty: Claude Code's default";
  const m = findModel(value, models);
  return m ? m.description : `Claude Code doesn't offer “${value}”: its runs will fail. Pick a model from the list.`;
}
