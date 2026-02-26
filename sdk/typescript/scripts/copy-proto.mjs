import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, "..");
const srcDir = path.join(root, "proto");
const destDir = path.join(root, "dist", "proto");

fs.mkdirSync(destDir, { recursive: true });
for (const file of fs.readdirSync(srcDir)) {
  if (!file.endsWith(".proto") && !file.endsWith(".json")) continue;
  fs.copyFileSync(path.join(srcDir, file), path.join(destDir, file));
}
console.log(`copied proto assets to ${destDir}`);
