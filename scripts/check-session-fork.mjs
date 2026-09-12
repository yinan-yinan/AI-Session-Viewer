import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { loadTypeScriptModule } from "./load-typescript-module.mjs";

const { createSessionFork } = await loadTypeScriptModule(
  fileURLToPath(new URL("../src/services/sessionFork.ts", import.meta.url)),
);
const result = { newSessionId: "new-id", newFilePath: "/sessions/new.jsonl", projectPath: "/project", projectId: "project" };

for (const source of ["claude", "codex", "grok", "omp"]) {
  const calls = [];
  const api = {
    async forkSession(...args) { calls.push(["fork", ...args]); return result; },
    async resumeSession(...args) { calls.push(["resume", ...args]); },
  };
  assert.deepEqual(await createSessionFork(api, source, "/old.jsonl", "target", false), { result, warning: null });
  assert.deepEqual(calls, [["fork", source, "/old.jsonl", "target"]], "Web/remote nodes must create a real fork without launching a terminal");
  calls.length = 0;
  await createSessionFork(api, source, "/old.jsonl", "target", true, "powershell");
  assert.deepEqual(calls, [
    ["fork", source, "/old.jsonl", "target"],
    ["resume", source, "new-id", "/project", "/sessions/new.jsonl", "powershell"],
  ], "Desktop must resume the newly-created identity in its own project");
}

let forkCount = 0;
const partial = await createSessionFork({
  async forkSession() { forkCount++; return result; },
  async resumeSession() { throw new Error("terminal unavailable"); },
}, "claude", "/old.jsonl", "target", true);
assert.equal(partial.result, result, "Terminal errors must preserve the created session");
assert.match(partial.warning, /terminal unavailable/);
assert.equal(forkCount, 1, "Terminal errors must not trigger another fork");

await assert.rejects(createSessionFork({
  async forkSession() { throw new Error("stale target"); },
  async resumeSession() { assert.fail("Creation failure must not resume any session"); },
}, "codex", "/old.jsonl", "target", true), /stale target/);
console.log("Session fork workflow checks passed.");
