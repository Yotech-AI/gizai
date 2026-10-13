// The brand's type, from the app's own fonts (src/assets/fonts): Geist for names, Atkinson Hyperlegible Next for text.
import { loadFont } from "@remotion/fonts";
import geist from "../../src/assets/fonts/Geist-var-latin.woff2";
import atkinson400 from "../../src/assets/fonts/AtkinsonHyperlegibleNext-400-latin.woff2";
import atkinson500 from "../../src/assets/fonts/AtkinsonHyperlegibleNext-500-latin.woff2";

export const NAME_FONT = "Geist";
export const TEXT_FONT = "Atkinson Hyperlegible Next";

loadFont({ family: NAME_FONT, url: geist, weight: "100 900" });
loadFont({ family: TEXT_FONT, url: atkinson400, weight: "400" });
loadFont({ family: TEXT_FONT, url: atkinson500, weight: "500" });
