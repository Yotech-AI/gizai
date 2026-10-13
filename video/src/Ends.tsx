// The tour's opening (the GizAi logo) and closing (the logo and "An Agentic Development Application" over the mountain
// band of the website's hero).
import type React from "react";
import { useEffect, useState } from "react";
import { AbsoluteFill, Img, cancelRender, continueRender, delayRender } from "remotion";
import logoOnDark from "../../design/brand/logo-horizontal-on-dark.svg";
import logoOnLight from "../../design/brand/logo-horizontal-on-light.svg";
import mountainsHero from "../../design/branding/mountains-hero.svg";
import { TEXT_FONT } from "./fonts";
import { COLORS, HEIGHT, type Theme } from "./scenes";

const LOGO = { dark: logoOnDark, light: logoOnLight };
/** The logo's own size is 358.58 × 90. */
const LOGO_RATIO = 90 / 358.58;

export const Opening: React.FC<{ theme: Theme; progress: number }> = ({ theme, progress }) => {
  const width = 1120;
  return (
    <AbsoluteFill style={{ background: COLORS[theme].bg, alignItems: "center", justifyContent: "center" }}>
      <Img src={LOGO[theme]} style={{ width, height: width * LOGO_RATIO, transform: `scale(${1 + 0.025 * progress})` }} />
    </AbsoluteFill>
  );
};

/** The hero's band: 3840 × 448 at 1:1 on the website, so 896 high here (scale 2), centred and cut at the sides. */
const BAND_HEIGHT = 896;

function useSvgText(url: string): string | null {
  const [text, setText] = useState<string | null>(null);
  const [handle] = useState(() => delayRender(`Loading ${url}`));
  useEffect(() => {
    fetch(url)
      .then((r) => r.text())
      .then((t) => {
        setText(t);
        continueRender(handle);
      })
      .catch((err) => cancelRender(err));
  }, [url, handle]);
  return text;
}

export const Closing: React.FC<{ theme: Theme; progress: number }> = ({ theme, progress }) => {
  const c = COLORS[theme];
  const mountains = useSvgText(mountainsHero);
  const width = 1000;
  const rise = (1 - progress) * 28;
  return (
    <AbsoluteFill style={{ background: c.bg }}>
      <div
        style={{
          position: "absolute",
          left: 0,
          right: 0,
          bottom: 0,
          height: BAND_HEIGHT,
          background: `linear-gradient(to bottom, transparent 20%, ${c.bandGlow})`,
        }}
      />
      {mountains && (
        <div
          className="gz-band"
          style={{ position: "absolute", left: 0, right: 0, bottom: 0, height: BAND_HEIGHT, color: c.bandLine }}
          // the file's own drawing; its lines are currentColor, 1px at 1:1, so 2px here
          dangerouslySetInnerHTML={{
            __html: `<style>.gz-band svg{display:block;width:100%;height:100%}.gz-band svg g{stroke-width:2px}</style>${mountains}`,
          }}
        />
      )}
      <div
        style={{
          position: "absolute",
          left: 0,
          right: 0,
          top: HEIGHT * 0.25,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 56,
          transform: `translateY(${rise}px)`,
        }}
      >
        <Img src={LOGO[theme]} style={{ width, height: width * LOGO_RATIO }} />
        <div style={{ fontFamily: TEXT_FONT, fontWeight: 500, fontSize: 64, color: c.text2, letterSpacing: "0.005em" }}>
          An Agentic Development Application
        </div>
      </div>
    </AbsoluteFill>
  );
};
