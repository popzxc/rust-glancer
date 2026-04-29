import * as vscode from "vscode";

export type TraceSetting = "off" | "messages" | "verbose";

export interface ExtensionConfig {
  readonly serverPath: string | undefined;
  readonly extraEnv: Record<string, string>;
  readonly traceServer: TraceSetting;
  readonly check: CheckConfig;
}

export interface CheckConfig {
  readonly onSave: boolean;
  readonly command: string;
  readonly arguments: string[];
}

export namespace ExtensionConfig {
  export function read(): ExtensionConfig {
    const config = vscode.workspace.getConfiguration("rust-glancer");
    const serverPath = config.get<string | null>("server.path", null);
    const extraEnv = config.get<Record<string, unknown>>("server.extraEnv", {});
    const traceServer = config.get<TraceSetting>("trace.server", "off");
    const checkOnSave = config.get<boolean>("checkOnSave", false);
    const checkCommand = config.get<string>("check.command", "check");
    const checkArguments = config.get<unknown[]>("check.arguments", ["--workspace", "--all-targets"]);

    return {
      serverPath: normalizeOptionalString(serverPath),
      extraEnv: normalizeStringRecord(extraEnv),
      traceServer,
      check: {
        onSave: checkOnSave,
        command: normalizeCargoSubcommand(checkCommand),
        arguments: normalizeStringArray(checkArguments),
      },
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

function normalizeStringArray(value: unknown[]): string[] {
  return value.filter((item): item is string => typeof item === "string");
}

function normalizeCargoSubcommand(value: string): string {
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : "check";
}
