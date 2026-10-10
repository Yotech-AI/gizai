import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { applyAppearance, getAppearance } from "./lib/appearance";
import "./styles/fonts.css";
import "./styles/tokens.css";
import "./styles/components.css";
import "./styles/app.css";
import "./styles/appearance.css";

// The font, text sizes, theme and density (Settings → Appearance), before anything is drawn.
applyAppearance(document.documentElement, getAppearance());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
