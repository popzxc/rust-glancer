import { spawn, type ChildProcess } from "child_process";
import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";
import type { ServerOptions } from "vscode-languageclient/node";

import type { ExtensionConfig } from "./config";

const SERVER_ENV_OVERRIDE = "__RUST_GLANCER_SERVER";

export interface ResolvedServer {
  readonly command: string;
  readonly args: readonly string[];
  readonly cwd: string;
  readonly env: NodeJS.ProcessEnv;
  readonly source: string;
}

export namespace ResolvedServer {
  export function discover(
    config: ExtensionConfig,
    workspaceFolder: vscode.WorkspaceFolder,
    extensionPath: string,
  ): ResolvedServer {
    if (config.serverPath !== undefined) {
      return executableServer(
        config.serverPath,
        "rust-glancer.server.path",
        config,
        workspaceFolder,
      );
    }

    const envServer = normalizeOptionalString(process.env[SERVER_ENV_OVERRIDE]);
    if (envServer !== undefined) {
      return executableServer(envServer, SERVER_ENV_OVERRIDE, config, workspaceFolder);
    }

    const repositoryRoot = path.resolve(extensionPath, "..", "..");
    if (isDevelopmentCheckout(repositoryRoot)) {
      return {
        command: "cargo",
        args: ["run", "--release", "-p", "rust-glancer", "--", "lsp"],
        cwd: repositoryRoot,
        env: buildEnv(config.extraEnv),
        source: "development checkout",
      };
    }

    return executableServer("rust-glancer", "PATH", config, workspaceFolder);
  }

  export function options(server: ResolvedServer, output: vscode.OutputChannel): ServerOptions {
    return (): Promise<ChildProcess> => {
      output.appendLine(`starting server: ${server.command} ${server.args.join(" ")}`);
      output.appendLine(`server cwd: ${server.cwd}`);
      output.appendLine(`server source: ${server.source}`);

      const child = spawn(server.command, [...server.args], {
        cwd: server.cwd,
        env: server.env,
        stdio: "pipe",
      });

      child.on("spawn", () => {
        output.appendLine(`server process started with pid ${child.pid ?? "unknown"}`);
      });

      child.on("error", (error) => {
        output.appendLine(`server failed to start: ${error.message}`);
        void vscode.window.showErrorMessage(
          `Failed to start rust-glancer language server: ${error.message}`,
        );
      });

      child.on("exit", (code, signal) => {
        output.appendLine(`server exited with code ${code ?? "null"} and signal ${signal ?? "null"}`);
      });

      return Promise.resolve(child);
    };
  }

  export function commandLine(server: ResolvedServer): string {
    return [server.command, ...server.args].join(" ");
  }
}

function executableServer(
  command: string,
  source: string,
  config: ExtensionConfig,
  workspaceFolder: vscode.WorkspaceFolder,
): ResolvedServer {
  return {
    command,
    args: ["lsp"],
    cwd: workspaceFolder.uri.fsPath,
    env: buildEnv(config.extraEnv),
    source,
  };
}

function buildEnv(extraEnv: Record<string, string>): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = { ...process.env };

  for (const [key, value] of Object.entries(extraEnv)) {
    env[key] = expandEnv(value, env);
  }

  return env;
}

function expandEnv(value: string, env: NodeJS.ProcessEnv): string {
  return value.replace(/\$([A-Za-z_][A-Za-z0-9_]*)|\$\{([^}]+)\}/g, (_, plain, braced) => {
    const key = plain ?? braced;
    return env[key] ?? "";
  });
}

function isDevelopmentCheckout(repositoryRoot: string): boolean {
  return (
    fs.existsSync(path.join(repositoryRoot, "Cargo.toml")) &&
    fs.existsSync(path.join(repositoryRoot, "crates", "rust-glancer", "Cargo.toml"))
  );
}

function normalizeOptionalString(value: string | undefined): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }

  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}
