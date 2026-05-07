// Note: We don't want to have a ton of tests for the extension.
// These are verbose and heavyweight, so we keep them mostly as a
// smoke test / basic e2e test.

import * as assert from "node:assert/strict";
import * as vscode from "vscode";

import { EXTENSION_COMMANDS } from "../src/commands";

const EXTENSION_ID = "rust-glancer.rust-glancer-code";

interface ClientStateSnapshot {
  readonly running: boolean;
  readonly hasClient: boolean;
  readonly status: {
    readonly state: string;
    readonly text: string;
    readonly details: {
      readonly workspaceRoot?: string;
      readonly serverCommand?: string;
      readonly serverSource?: string;
    };
  };
}

suite("Rust Glancer extension", () => {
  test("activates on a Rust workspace and reindexes with the real server", async () => {
    const extension = vscode.extensions.getExtension(EXTENSION_ID);
    assert.ok(extension, `expected VS Code to load extension ${EXTENSION_ID}`);

    await extension.activate();

    const ready = await waitForClientState(
      (state) => state.running && state.status.state === "ready",
    );
    assert.equal(ready.hasClient, true);
    assert.equal(ready.status.text, "$(check) Rust Glancer: ready");
    assert.match(ready.status.details.workspaceRoot ?? "", /test_targets[/\\]simple_crate$/);

    const commands = await vscode.commands.getCommands(true);
    assert.ok(commands.includes(EXTENSION_COMMANDS.restartServer));
    assert.ok(commands.includes(EXTENSION_COMMANDS.reindexWorkspace));

    await vscode.commands.executeCommand(EXTENSION_COMMANDS.reindexWorkspace);

    const reindexed = await waitForClientState(
      (state) => state.running && state.status.state === "ready",
    );
    assert.equal(reindexed.hasClient, true);
  });
});

async function waitForClientState(
  isExpected: (state: ClientStateSnapshot) => boolean,
): Promise<ClientStateSnapshot> {
  const startedAt = Date.now();
  let lastState: ClientStateSnapshot | undefined;

  while (Date.now() - startedAt < 30_000) {
    lastState = await vscode.commands.executeCommand<ClientStateSnapshot>(
      EXTENSION_COMMANDS.testGetState,
    );
    if (lastState !== undefined && isExpected(lastState)) {
      return lastState;
    }

    await delay(100);
  }

  assert.fail(
    `timed out waiting for rust-glancer extension state; last state: ${JSON.stringify(lastState)}`,
  );
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
