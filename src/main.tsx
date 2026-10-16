import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { onboardingPending } from "./api";
import { applyAppearance, getAppearance } from "./lib/appearance";
import "./styles/fonts.css";
import "./styles/tokens.css";
import "./styles/components.css";
import "./styles/app.css";
import "./styles/changes.css";
import "./styles/screenshots.css";
import "./styles/terminal.css";
import "./styles/appearance.css";
import "./styles/band.css";

// The font, text sizes, theme, density and mountains (Settings → Appearance), before anything is drawn.
applyAppearance(document.documentElement, getAppearance());

// GA-164: a new install opens on the first start's onboarding. Gizai asks whether it waits before drawing anything, so the
// board doesn't show for a moment first; when that can't be asked, the app opens as it always did.
onboardingPending().catch(() => false).then((onboarding) => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App onboarding={onboarding} />
    </React.StrictMode>,
  );
});
