import * as vscode from "vscode";

import { ClientManager } from "./client";
import { StatusView } from "./status";

let manager: ClientManager | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const output = vscode.window.createOutputChannel("Rust Glimpser");
  const status = new StatusView();
  manager = new ClientManager(context.extensionPath, output, status);

  context.subscriptions.push(
    output,
    status,
    manager,
    vscode.commands.registerCommand("rust-glimpser.restartServer", async () => {
      await manager?.restart();
    }),
  );

  await manager.start();
}

export async function deactivate(): Promise<void> {
  await manager?.stop();
  manager = undefined;
}
