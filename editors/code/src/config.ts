import * as vscode from "vscode";

export type TraceSetting = "off" | "messages" | "verbose";

export interface ExtensionConfig {
  readonly serverPath: string | undefined;
  readonly extraEnv: Record<string, string>;
  readonly traceServer: TraceSetting;
}

export namespace ExtensionConfig {
  export function read(): ExtensionConfig {
    const config = vscode.workspace.getConfiguration("rust-glimpser");
    const serverPath = config.get<string | null>("server.path", null);
    const extraEnv = config.get<Record<string, unknown>>("server.extraEnv", {});
    const traceServer = config.get<TraceSetting>("trace.server", "off");

    return {
      serverPath: normalizeOptionalString(serverPath),
      extraEnv: normalizeStringRecord(extraEnv),
      traceServer,
    };
  }
}

function normalizeOptionalString(value: string | null): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }

  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

function normalizeStringRecord(value: Record<string, unknown>): Record<string, string> {
  const result: Record<string, string> = {};

  // VS Code settings are user-editable JSON. Keep the runtime boundary strict
  // and ignore malformed entries rather than failing extension activation.
  for (const [key, envValue] of Object.entries(value)) {
    if (typeof envValue === "string") {
      result[key] = envValue;
    }
  }

  return result;
}
