// CI: the committed fixtures must have the shape the CLI answers with today. Values differ on every run (ids, times); keys do not.
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const committed = join(here, "..", "src", "test", "fixtures");
const fresh = mkdtempSync(join(tmpdir(), "tracewise-fixtures-"));
execFileSync("sh", [join(here, "refresh-fixtures.sh"), fresh], { stdio: "inherit", env: process.env });

/** The keys of a JSON value, recursively; arrays are described by their first element. */
function shape(value) {
  if (Array.isArray(value)) return value.length ? [shape(value[0])] : [];
  if (value && typeof value === "object") return Object.fromEntries(Object.keys(value).sort().map((k) => [k, shape(value[k])]));
  return typeof value === "string" ? "string" : value === null ? "null" : typeof value;
}

let bad = 0;
for (const file of readdirSync(committed).filter((f) => f.endsWith(".json"))) {
  const a = JSON.stringify(shape(JSON.parse(readFileSync(join(committed, file), "utf8"))));
  const b = JSON.stringify(shape(JSON.parse(readFileSync(join(fresh, file), "utf8"))));
  if (a !== b) {
    bad++;
    console.error(`${file}: the CLI's answer has a different shape; run \`pnpm fixtures\` and commit the result`);
  }
}
process.exit(bad ? 1 : 0);
