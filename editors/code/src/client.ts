import * as vscode from "vscode";
import {
  ExecuteCommandRequest,
  LanguageClient,
  State,
  type LanguageClientOptions,
  Trace,
} from "vscode-languageclient/node";

import { SERVER_COMMANDS } from "./commands";
import { ExtensionConfig, type TraceSetting } from "./config";
import { ClientStatus, type ClientStatusSnapshot } from "./client_status";
import { hoverMiddleware } from "./hover_actions";
import { ResolvedServer } from "./server";
import { StatusView } from "./status";

export interface ClientManagerSnapshot extends ClientStatusSnapshot {
  readonly hasClient: boolean;
}

export class ClientManager implements vscode.Disposable {
  private client: LanguageClient | undefined;
  private clientState: vscode.Disposable | undefined;
  private readonly clientStatus: ClientStatus;
  private readonly editorStateListeners: vscode.Disposable;

  public constructor(
    private readonly extensionPath: string,
    private readonly output: vscode.OutputChannel,
    status: StatusView,
  ) {
    this.clientStatus = new ClientStatus(status);
    this.editorStateListeners = vscode.Disposable.from(
      vscode.window.onDidChangeActiveTextEditor(() => this.updateDocumentFreshnessStatus()),
      vscode.workspace.onDidChangeTextDocument((event) => {
        if (this.isRustFile(event.document)) {
          this.updateDocumentFreshnessStatus();
        }
      }),
      vscode.workspace.onDidSaveTextDocument((document) => {
        if (this.isRustFile(document)) {
          this.updateDocumentFreshnessStatus();
        }
      }),
      vscode.workspace.onDidCloseTextDocument((document) => {
        if (this.isRustFile(document)) {
          this.updateDocumentFreshnessStatus();
        }
      }),
    );
  }

  public async start(): Promise<void> {
    if (this.client !== undefined) {
      return;
    }

    const workspaceFolder = await this.workspaceFolder();
    if (workspaceFolder === undefined) {
      this.output.appendLine("no Cargo workspace folder found; rust-glancer server was not started");
      this.clientStatus.stopped("no Cargo workspace folder");
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
    this.clientStatus.starting(statusDetails);

    const clientOptions: LanguageClientOptions = {
      documentSelector: [
        {
          scheme: "file",
          language: "rust",
        },
      ],
      diagnosticCollectionName: "rust-glancer",
      outputChannel: this.output,
      traceOutputChannel: this.output,
      initializationOptions: {
        check: config.check,
        cargo: config.cargo,
        cache: config.cache,
      },
      middleware: this.middleware(),
      workspaceFolder,
    };

    const client = new LanguageClient(
      "rust-glancer",
      "Rust Glancer",
      ResolvedServer.options(server, this.output),
      clientOptions,
    );

    this.client = client;
    this.clientState = client.onDidChangeState((event) => {
      switch (event.newState) {
        case State.Starting:
          this.clientStatus.starting(statusDetails);
          break;
        case State.Running:
          this.clientStatus.ready(statusDetails);
          this.updateDocumentFreshnessStatus();
          break;
        case State.Stopped:
          if (this.client === client) {
            this.clientStatus.stopped("language client stopped", statusDetails);
          }
          break;
      }
    });

    try {
      await client.start();
      await client.setTrace(trace(config.traceServer));
      this.clientStatus.ready(statusDetails);
      this.updateDocumentFreshnessStatus();
      this.output.appendLine("rust-glancer client started");
    } catch (error) {
      this.client = undefined;
      this.clientState?.dispose();
      this.clientState = undefined;
      this.clientStatus.failed(String(error), statusDetails);
      this.output.appendLine(`rust-glancer client failed to start: ${String(error)}`);
      void vscode.window.showErrorMessage(
        "Rust Glancer failed to start. Check the Rust Glancer output for details.",
      );
    }
  }

  public async restart(): Promise<void> {
    this.output.appendLine("restarting rust-glancer server");
    await this.stop();
    await this.start();
  }

  public async reindexWorkspace(): Promise<void> {
    const client = this.client;
    if (!this.clientStatus.isRunning() || client === undefined) {
      void vscode.window.showWarningMessage("Rust Glancer is not running.");
      return;
    }

    this.output.appendLine("reindexing rust-glancer workspace");
    this.clientStatus.indexing();

    try {
      await client.sendRequest(ExecuteCommandRequest.type, {
        command: SERVER_COMMANDS.reindexWorkspace,
        arguments: [],
      });
      this.output.appendLine("rust-glancer workspace reindex finished");
      this.updateDocumentFreshnessStatus();
    } catch (error) {
      this.output.appendLine(`rust-glancer workspace reindex failed: ${String(error)}`);
      this.clientStatus.operationFailed(`reindex failed: ${String(error)}`);
      void vscode.window.showErrorMessage(
        "Rust Glancer failed to reindex the workspace. Check the Rust Glancer output for details.",
      );
    }
  }

  public async stop(): Promise<void> {
    const client = this.client;
    this.client = undefined;
    this.clientState?.dispose();
    this.clientState = undefined;

    if (client !== undefined) {
      await client.stop();
      this.output.appendLine("rust-glancer client stopped");
    }

    this.clientStatus.stopped("not running");
  }

  public snapshot(): ClientManagerSnapshot {
    const status = this.clientStatus.snapshot();
    return {
      hasClient: this.client !== undefined,
      ...status,
    };
  }

  public dispose(): void {
    this.editorStateListeners.dispose();
    void this.stop();
  }

  private updateDocumentFreshnessStatus(): void {
    this.clientStatus.refresh(this.isActiveRustDocumentDirty());
  }

  private middleware(): LanguageClientOptions["middleware"] {
    return {
      ...hoverMiddleware(() => this.client, this.output),
      handleWorkDoneProgress: (token, params, next) => {
        this.clientStatus.handleWorkDoneProgress(
          token,
          params,
          this.isActiveRustDocumentDirty(),
        );
        next(token, params);
      },
    };
  }

  private isActiveRustDocumentDirty(): boolean {
    const document = vscode.window.activeTextEditor?.document;
    return document !== undefined && this.isRustFile(document) && document.isDirty;
  }

  private isRustFile(document: vscode.TextDocument): boolean {
    return document.uri.scheme === "file" && document.languageId === "rust";
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
        "multiple workspace folders detected, but none contains Cargo.toml; rust-glancer server was not started",
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
