import * as vscode from "vscode";

export interface StatusDetails {
  readonly workspaceRoot?: string;
  readonly serverCommand?: string;
  readonly serverSource?: string;
}

export class StatusView implements vscode.Disposable {
  private readonly item: vscode.StatusBarItem;
  private details: StatusDetails = {};
  private disposed = false;

  public constructor() {
    this.item = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
    this.item.name = "Rust Glancer";
    this.item.command = "rust-glancer.restartServer";
  }

  public starting(details: StatusDetails): void {
    if (this.disposed) {
      return;
    }

    this.details = details;
    this.item.text = "$(sync~spin) Rust Glancer";
    this.item.tooltip = this.tooltip("Starting");
    this.item.backgroundColor = undefined;
    this.item.show();
  }

  public ready(details: StatusDetails = this.details): void {
    if (this.disposed) {
      return;
    }

    this.details = details;
    this.item.text = "$(check) Rust Glancer";
    this.item.tooltip = this.tooltip("Ready");
    this.item.backgroundColor = undefined;
    this.item.show();
  }

  public stale(details: StatusDetails = this.details): void {
    if (this.disposed) {
      return;
    }

    this.details = details;
    this.item.text = "$(warning) Rust Glancer";
    this.item.tooltip = this.tooltip("Stale: save to update");
    this.item.backgroundColor = undefined;
    this.item.show();
  }

  public stopped(reason: string, details: StatusDetails = this.details): void {
    if (this.disposed) {
      return;
    }

    this.details = details;
    this.item.text = "$(circle-slash) Rust Glancer";
    this.item.tooltip = this.tooltip(`Stopped: ${reason}`);
    this.item.backgroundColor = undefined;
    this.item.show();
  }

  public failed(reason: string, details: StatusDetails = this.details): void {
    if (this.disposed) {
      return;
    }

    this.details = details;
    this.item.text = "$(error) Rust Glancer";
    this.item.tooltip = this.tooltip(`Failed: ${reason}`);
    this.item.backgroundColor = new vscode.ThemeColor("statusBarItem.errorBackground");
    this.item.show();
  }

  public dispose(): void {
    this.disposed = true;
    this.item.dispose();
  }

  private tooltip(state: string): vscode.MarkdownString {
    const tooltip = new vscode.MarkdownString();
    tooltip.appendMarkdown(`**Rust Glancer**\n\n`);
    appendTextField(tooltip, "State", state);

    if (this.details.workspaceRoot !== undefined) {
      appendCodeField(tooltip, "Workspace", this.details.workspaceRoot);
    }
    if (this.details.serverCommand !== undefined) {
      appendCodeField(tooltip, "Server", this.details.serverCommand);
    }
    if (this.details.serverSource !== undefined) {
      appendTextField(tooltip, "Source", this.details.serverSource);
    }

    tooltip.appendMarkdown("Click to restart the server.");
    return tooltip;
  }
}

function appendTextField(tooltip: vscode.MarkdownString, label: string, value: string): void {
  tooltip.appendMarkdown(`${label}: `);
  tooltip.appendText(singleLine(value));
  tooltip.appendMarkdown("\n\n");
}

function appendCodeField(tooltip: vscode.MarkdownString, label: string, value: string): void {
  tooltip.appendMarkdown(`${label}: \``);
  tooltip.appendText(singleLine(value));
  tooltip.appendMarkdown("`\n\n");
}

function singleLine(value: string): string {
  return value.replace(/\s+/g, " ");
}
