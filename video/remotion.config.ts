// Remotion's settings for `npx remotion studio` and `npx remotion render` (https://www.remotion.dev/docs/config).
import { Config } from "@remotion/cli/config";

// The stills stay where GA-148's capture wrote them: capture/<theme>/<scene>/<step>-before.png is
// staticFile("<theme>/<scene>/<step>-before.png").
Config.setPublicDir("./capture");
Config.setEntryPoint("./src/index.ts");
// The frames go to the encoder as JPEG at 95: much faster than PNG at 2560 × 1664, and the text stays sharp.
Config.setVideoImageFormat("jpeg");
Config.setJpegQuality(95);
Config.setOverwriteOutput(true);
