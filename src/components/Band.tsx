// The mountains along the bottom of the window (GA-139's design, GA-143): the window's lowest layer, .app's first child
// while Settings → Appearance → "Show mountains along the bottom" is on. band.css places it and says which surfaces are
// see-through over it.
import mountains from "../assets/mountains.svg?raw";
import { useAppearance } from "../lib/appearance";

/** The artwork inline, not an <img>: so its lines take currentColor (band-line) in both themes. It takes no clicks and
 *  screen readers skip it. It follows the setting itself, so turning it on or off doesn't redraw the rest of the app. */
export function Band() {
  const { band } = useAppearance();
  if (!band) return null;
  return <div className="band" aria-hidden="true" dangerouslySetInnerHTML={{ __html: mountains }} />;
}
