import * as vscode from "vscode";
import {
  LanguageClient,
  State,
  type LanguageClientOptions,
  Trace,
} from "vscode-languageclient/node";

import { ExtensionConfig, type TraceSetting } from "./config";
import { ResolvedServer } from "./server";
import { StatusView } from "./status";

export class ClientManager implements vscode.Disposable {
  private client: LanguageClient | undefined;
  private clientState: vscode.Disposable | undefined;

  public constructor(
    private readonly extensionPath: string,
    private readonly output: vscode.OutputChannel,
    private readonly status: StatusView,
  ) {}

  public async start(): Promise<void> {
    if (this.client !== undefined) {
      return;
    }

    const workspaceFolder = await this.workspaceFolder();
    if (workspaceFolder === undefined) {
      this.output.appendLine("no Cargo workspace folder found; rust-glimpser server was not started");
      this.status.stopped("no Cargo workspace folder");
      return;
    }

    const config = ExtensionConfig.read();
    const server = ResolvedServer.discover(config, workspaceFolder, this.extensionPath);
    const statusDetails = {
      workspaceRoot: workspaceFolder.uri.fsPath,
      serverCommand: ResolvedServer.commandLine(server),
      serverSource: server.source,
    };

    this.output.appendLine(`workspace root: ${workspaceFolder.uri.fsPath}`);
    this.output.appendLine(`server command: ${statusDetails.serverCommand}`);
    this.output.appendLine(`server source: ${statusDetails.serverSource}`);
    this.status.starting(statusDetails);

    const clientOptions: LanguageClientOptions = {
      documentSelector: [
        {
          scheme: "file",
          language: "rust",
          pattern: `${workspaceFolder.uri.fsPath.replace(/\\/g, "/")}/**/*.rs`,
        },
      ],
      diagnosticCollectionName: "rust-glimpser",
      outputChannel: this.output,
      traceOutputChannel: this.output,
      workspaceFolder,
    };

    const client = new LanguageClient(
      "rust-glimpser",
      "Rust Glimpser",
      ResolvedServer.options(server, this.output),
      clientOptions,
    );

    this.client = client;
    this.clientState = client.onDidChangeState((event) => {
      switch (event.newState) {
        case State.Starting:
          this.status.starting(statusDetails);
          break;
        case State.Running:
          this.status.ready(statusDetails);
          break;
        case State.Stopped:
          if (this.client === client) {
            this.status.stopped("language client stopped", statusDetails);
          }
          break;
      }
    });

    try {
      await client.start();
      await client.setTrace(trace(config.traceServer));
      this.status.ready(statusDetails);
      this.output.appendLine("rust-glimpser client started");
    } catch (error) {
      this.client = undefined;
      this.clientState?.dispose();
      this.clientState = undefined;
      this.status.failed(String(error), statusDetails);
      this.output.appendLine(`rust-glimpser client failed to start: ${String(error)}`);
      void vscode.window.showErrorMessage(
        "Rust Glimpser failed to start. Check the Rust Glimpser output for details.",
      );
    }
  }

  public async restart(): Promise<void> {
    this.output.appendLine("restarting rust-glimpser server");
    await this.stop();
    await this.start();
  }

  public async stop(): Promise<void> {
    const client = this.client;
    this.client = undefined;
    this.clientState?.dispose();
    this.clientState = undefined;

    if (client !== undefined) {
      await client.stop();
      this.output.appendLine("rust-glimpser client stopped");
    }

    this.status.stopped("not running");
  }

  public dispose(): void {
    void this.stop();
  }

  private async workspaceFolder(): Promise<vscode.WorkspaceFolder | undefined> {
    const activeUri = vscode.window.activeTextEditor?.document.uri;
    if (activeUri?.scheme === "file") {
      const activeWorkspace = vscode.workspace.getWorkspaceFolder(activeUri);
      if (activeWorkspace !== undefined) {
        this.output.appendLine(`using active editor workspace folder: ${activeWorkspace.uri.fsPath}`);
        return activeWorkspace;
      }
    }

    const folders = vscode.workspace.workspaceFolders ?? [];
    for (const folder of folders) {
      if (await hasCargoManifest(folder)) {
        if (folders.length > 1) {
          this.output.appendLine(
            `multiple workspace folders detected; using first folder with Cargo.toml: ${folder.uri.fsPath}`,
          );
        }

        return folder;
      }
    }

    if (folders.length > 1) {
      this.output.appendLine(
        "multiple workspace folders detected, but none contains Cargo.toml; rust-glimpser server was not started",
      );
    }

    return undefined;
  }
}

async function hasCargoManifest(folder: vscode.WorkspaceFolder): Promise<boolean> {
  if (folder.uri.scheme !== "file") {
    return false;
  }

  try {
    await vscode.workspace.fs.stat(vscode.Uri.joinPath(folder.uri, "Cargo.toml"));
    return true;
  } catch {
    return false;
  }
}

function trace(setting: TraceSetting): Trace {
  switch (setting) {
    case "off":
      return Trace.Off;
    case "messages":
      return Trace.Messages;
    case "verbose":
      return Trace.Verbose;
  }
}
