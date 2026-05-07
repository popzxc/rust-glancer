import * as vscode from "vscode";
import {
  ExecuteCommandRequest,
  LanguageClient,
  State,
  type ProgressToken,
  type LanguageClientOptions,
  Trace,
  type WorkDoneProgressBegin,
  type WorkDoneProgressEnd,
  type WorkDoneProgressReport,
} from "vscode-languageclient/node";

import { ExtensionConfig, type TraceSetting } from "./config";
import { hoverMiddleware } from "./hover_actions";
import { ResolvedServer } from "./server";
import { StatusView, type StatusDetails } from "./status";

const REINDEX_WORKSPACE_COMMAND = "rust-glancer.internal.reindexWorkspace";
const CARGO_DIAGNOSTICS_PROGRESS_TITLE = "Cargo diagnostics";

export class ClientManager implements vscode.Disposable {
  private client: LanguageClient | undefined;
  private clientState: vscode.Disposable | undefined;
  private currentStatusDetails: StatusDetails | undefined;
  private running = false;
  private checkRunning = false;
  private checkFailed = false;
  private checkCommand: string | undefined;
  private readonly checkProgressTokens = new Set<ProgressToken>();
  private readonly editorStateListeners: vscode.Disposable;

  public constructor(
    private readonly extensionPath: string,
    private readonly output: vscode.OutputChannel,
    private readonly status: StatusView,
  ) {
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
    this.currentStatusDetails = statusDetails;

    this.output.appendLine(`workspace root: ${workspaceFolder.uri.fsPath}`);
    this.output.appendLine(`server command: ${statusDetails.serverCommand}`);
    this.output.appendLine(`server source: ${statusDetails.serverSource}`);
    this.status.starting(statusDetails);

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
          this.running = false;
          this.status.starting(statusDetails);
          break;
        case State.Running:
          this.running = true;
          this.status.ready(statusDetails);
          this.updateDocumentFreshnessStatus();
          break;
        case State.Stopped:
          this.running = false;
          if (this.client === client) {
            this.status.stopped("language client stopped", statusDetails);
          }
          break;
      }
    });

    try {
      await client.start();
      await client.setTrace(trace(config.traceServer));
      this.running = true;
      this.status.ready(statusDetails);
      this.updateDocumentFreshnessStatus();
      this.output.appendLine("rust-glancer client started");
    } catch (error) {
      this.client = undefined;
      this.clientState?.dispose();
      this.clientState = undefined;
      this.running = false;
      this.checkRunning = false;
      this.checkFailed = false;
      this.checkCommand = undefined;
      this.status.failed(String(error), statusDetails);
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
    if (!this.running || client === undefined || this.currentStatusDetails === undefined) {
      void vscode.window.showWarningMessage("Rust Glancer is not running.");
      return;
    }

    this.output.appendLine("reindexing rust-glancer workspace");
    this.status.indexing(this.currentStatusDetails);

    try {
      await client.sendRequest(ExecuteCommandRequest.type, {
        command: REINDEX_WORKSPACE_COMMAND,
        arguments: [],
      });
      this.output.appendLine("rust-glancer workspace reindex finished");
      this.updateStatus();
    } catch (error) {
      this.output.appendLine(`rust-glancer workspace reindex failed: ${String(error)}`);
      this.status.failed(`reindex failed: ${String(error)}`, this.currentStatusDetails);
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
    this.running = false;
    this.checkRunning = false;
    this.checkFailed = false;
    this.checkCommand = undefined;
    this.checkProgressTokens.clear();

    if (client !== undefined) {
      await client.stop();
      this.output.appendLine("rust-glancer client stopped");
    }

    this.status.stopped("not running");
  }

  public dispose(): void {
    this.editorStateListeners.dispose();
    void this.stop();
  }

  private updateDocumentFreshnessStatus(): void {
    this.updateStatus();
  }

  private updateStatus(): void {
    if (!this.running || this.currentStatusDetails === undefined) {
      return;
    }

    const document = vscode.window.activeTextEditor?.document;
    if (document !== undefined && this.isRustFile(document) && document.isDirty) {
      this.status.stale(this.currentStatusDetails);
    } else if (this.checkRunning) {
      this.status.checkRunning(this.checkCommand, this.currentStatusDetails);
    } else if (this.checkFailed) {
      this.status.checkFailed(this.currentStatusDetails);
    } else {
      this.status.ready(this.currentStatusDetails);
    }
  }

  private middleware(): LanguageClientOptions["middleware"] {
    return {
      ...hoverMiddleware(() => this.client, this.output),
      handleWorkDoneProgress: (token, params, next) => {
        this.handleWorkDoneProgress(token, params);
        next(token, params);
      },
    };
  }

  private handleWorkDoneProgress(
    token: ProgressToken,
    params: WorkDoneProgressBegin | WorkDoneProgressReport | WorkDoneProgressEnd,
  ): void {
    if (params.kind === "begin") {
      if (params.title !== CARGO_DIAGNOSTICS_PROGRESS_TITLE) {
        return;
      }

      this.checkProgressTokens.add(token);
      this.checkRunning = true;
      this.checkFailed = false;
      this.checkCommand = params.message;
      this.updateStatus();
      return;
    }

    if (!this.checkProgressTokens.has(token)) {
      return;
    }

    if (params.kind === "end") {
      this.checkProgressTokens.delete(token);
      this.checkRunning = this.checkProgressTokens.size > 0;
      this.checkFailed = params.message === "Failed";
      if (!this.checkRunning) {
        this.checkCommand = undefined;
      }
      this.updateStatus();
    }
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
