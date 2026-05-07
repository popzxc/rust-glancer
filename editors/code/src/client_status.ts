import {
  type ProgressToken,
  type WorkDoneProgressBegin,
  type WorkDoneProgressEnd,
  type WorkDoneProgressReport,
} from "vscode-languageclient/node";

import { StatusView, type StatusDetails, type StatusSnapshot } from "./status";

const CARGO_DIAGNOSTICS_PROGRESS_TITLE = "Cargo diagnostics";

export interface ClientStatusSnapshot {
  readonly running: boolean;
  readonly checkRunning: boolean;
  readonly checkFailed: boolean;
  readonly checkCommand: string | undefined;
  readonly status: StatusSnapshot;
  readonly details: StatusDetails | undefined;
}

/**
 * Tracks client-facing state and decides which status-bar state should win.
 *
 * VS Code document events, LSP lifecycle events, and work-done progress can arrive independently.
 * Keeping their merge logic here makes `ClientManager` mostly responsible for wiring.
 */
export class ClientStatus {
  private details: StatusDetails | undefined;
  private running = false;
  private checkRunning = false;
  private checkFailed = false;
  private checkCommand: string | undefined;
  private readonly checkProgressTokens = new Set<ProgressToken>();

  public constructor(private readonly view: StatusView) {}

  public isRunning(): boolean {
    return this.running;
  }

  public currentDetails(): StatusDetails | undefined {
    return this.details === undefined ? undefined : { ...this.details };
  }

  public starting(details: StatusDetails): void {
    this.running = false;
    this.resetCheck();
    this.details = details;
    this.view.starting(details);
  }

  public ready(details: StatusDetails): void {
    this.running = true;
    this.details = details;
    this.view.ready(details);
  }

  public indexing(): void {
    if (this.details === undefined) {
      return;
    }

    this.view.indexing(this.details);
  }

  public stopped(reason: string, details: StatusDetails | undefined = this.details): void {
    this.running = false;
    this.resetCheck();
    this.details = details;
    this.view.stopped(reason, details ?? {});
  }

  public failed(reason: string, details: StatusDetails | undefined = this.details): void {
    this.running = false;
    this.resetCheck();
    this.details = details;
    this.view.failed(reason, details ?? {});
  }

  public operationFailed(reason: string): void {
    if (this.details === undefined) {
      return;
    }

    // A failed request is user-visible, but it does not necessarily mean the LSP client stopped.
    this.view.failed(reason, this.details);
  }

  public refresh(isActiveRustDocumentDirty: boolean): void {
    if (!this.running || this.details === undefined) {
      return;
    }

    // Dirty buffers are shown first because the last published analysis no longer describes
    // the file the user is looking at. Cargo diagnostics remain visible once the editor is clean.
    if (isActiveRustDocumentDirty) {
      this.view.stale(this.details);
    } else if (this.checkRunning) {
      this.view.checkRunning(this.checkCommand, this.details);
    } else if (this.checkFailed) {
      this.view.checkFailed(this.details);
    } else {
      this.view.ready(this.details);
    }
  }

  public handleWorkDoneProgress(
    token: ProgressToken,
    params: WorkDoneProgressBegin | WorkDoneProgressReport | WorkDoneProgressEnd,
    isActiveRustDocumentDirty: boolean,
  ): void {
    if (params.kind === "begin") {
      if (params.title !== CARGO_DIAGNOSTICS_PROGRESS_TITLE) {
        return;
      }

      this.checkProgressTokens.add(token);
      this.checkRunning = true;
      this.checkFailed = false;
      this.checkCommand = params.message;
      this.refresh(isActiveRustDocumentDirty);
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
      this.refresh(isActiveRustDocumentDirty);
    }
  }

  public snapshot(): ClientStatusSnapshot {
    return {
      running: this.running,
      checkRunning: this.checkRunning,
      checkFailed: this.checkFailed,
      checkCommand: this.checkCommand,
      status: this.view.snapshot(),
      details: this.currentDetails(),
    };
  }

  private resetCheck(): void {
    this.checkRunning = false;
    this.checkFailed = false;
    this.checkCommand = undefined;
    this.checkProgressTokens.clear();
  }
}
