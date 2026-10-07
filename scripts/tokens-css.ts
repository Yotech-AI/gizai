// Regenerates src/styles/tokens.css from design/tokens.json:  node scripts/tokens-css.ts
import { readFileSync, writeFileSync } from "node:fs";
import { tokensCss, type Tokens } from "../src/lib/tokensCss.ts";

const root = new URL("..", import.meta.url).pathname;
const tokens = JSON.parse(readFileSync(`${root}design/tokens.json`, "utf8")) as Tokens;
writeFileSync(`${root}src/styles/tokens.css`, tokensCss(tokens));
console.log("wrote src/styles/tokens.css");
