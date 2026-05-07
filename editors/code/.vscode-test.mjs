import { defineConfig } from "@vscode/test-cli";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const extensionRoot = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  files: "out/test/**/*.test.js",
  version: "1.119.0",
  extensionDevelopmentPath: extensionRoot,
  workspaceFolder: resolve(extensionRoot, "../../test_targets/simple_crate"),
  launchArgs: ["--disable-extensions"],
  mocha: {
    timeout: 60_000,
  },
});
