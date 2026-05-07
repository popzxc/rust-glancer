import * as vscode from "vscode";

import { ClientManager } from "./client";
import { registerHoverActionCommands } from "./hover_actions";
import { StatusView } from "./status";

let manager: ClientManager | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const output = vscode.window.createOutputChannel("Rust Glancer");
  const status = new StatusView();
  manager = new ClientManager(context.extensionPath, output, status);

  context.subscriptions.push(
    output,
    status,
    manager,
    registerHoverActionCommands(output),
    vscode.commands.registerCommand("rust-glancer.restartServer", async () => {
      await manager?.restart();
    }),
    vscode.commands.registerCommand("rust-glancer.reindexWorkspace", async () => {
      await manager?.reindexWorkspace();
    }),
  );

  await manager.start();
}

export async function deactivate(): Promise<void> {
  await manager?.stop();
  manager = undefined;
}
