// A scene's caption at the bottom left: the feature's number and name from the website (Geist), and one short line
// (Atkinson Hyperlegible Next).
import type React from "react";
import { NAME_FONT, TEXT_FONT } from "./fonts";
import { COLORS, type Caption as CaptionText, type Theme } from "./scenes";

export const Caption: React.FC<{ caption: CaptionText; theme: Theme; opacity: number; lift: number }> = ({ caption, theme, opacity, lift }) => {
  const c = COLORS[theme];
  return (
    <div
      style={{
        position: "absolute",
        left: 48,
        bottom: 44,
        maxWidth: 1480,
        padding: "26px 40px 30px",
        borderRadius: 22,
        background: c.raised,
        border: `2px solid ${c.line}`,
        boxShadow: c.shadow,
        opacity,
        transform: `translateY(${lift}px)`,
      }}
    >
      <div style={{ display: "flex", alignItems: "baseline", gap: 22, fontFamily: NAME_FONT, letterSpacing: "-0.01em" }}>
        <span style={{ color: c.pink, fontSize: 40, fontWeight: 600, fontVariantNumeric: "tabular-nums" }}>{caption.number}</span>
        <span style={{ color: c.text, fontSize: 56, fontWeight: 600, lineHeight: 1.15 }}>{caption.name}</span>
      </div>
      <div style={{ marginTop: 8, color: c.text2, fontFamily: TEXT_FONT, fontSize: 36, lineHeight: 1.3 }}>{caption.line}</div>
    </div>
  );
};
