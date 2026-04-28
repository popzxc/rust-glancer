# Rust Glimpser VS Code Extension

This extension starts `rust-glimpser lsp` over stdio and wires VS Code to the
current saved-file-only language server.

## Development

From `editors/code`:

```text
npm install
npm run compile
```

Then launch the extension development host from VS Code.

The development fallback starts the server with:

```text
cargo run --release -p rust-glimpser -- lsp
```

When the extension is not running from this repository checkout, it falls back
to an installed `rust-glimpser` binary.

### Launching

There are two supported development launch flows.

From the repository root:

1. Open the repository root in VS Code.
2. Run `npm install` once from `editors/code`.
3. Start the `Run Rust Glimpser Extension` launch configuration.
4. The new Extension Development Host window opens the repository root as the
   Rust workspace.

From the extension folder:

1. Open `editors/code` as the workspace folder in VS Code.
2. Run `npm install` once.
3. Start the `Run Rust Glimpser Extension` launch configuration.
4. The new Extension Development Host window opens the repository root as the
   Rust workspace.

The first server startup may take longer because Cargo builds the release
binary. Later restarts reuse Cargo's release artifacts.

When the extension is active, VS Code shows a `Rust Glimpser` status bar item.
It displays startup, ready, stopped, and failed states; hover it to see the
workspace root and server command, or click it to restart the server.

If the Extension Development Host opens but `Rust Glimpser` is missing from
the Output panel and the command palette, VS Code probably launched the wrong
extension development path. Use one of the launch configurations above rather
than pressing F5 from `src/extension.ts` without an extension launch config.

If VS Code reports `Cannot call write after a stream was destroyed`, the server
process exited during startup. Open the `Rust Glimpser` output channel; the
extension logs the exact command, working directory, process exit, and server
stderr there.

## Settings

```json
{
  "rust-glimpser.server.path": null,
  "rust-glimpser.server.extraEnv": {},
  "rust-glimpser.trace.server": "off"
}
```

`rust-glimpser.server.path` should point to the `rust-glimpser` executable
itself; the extension adds the `lsp` subcommand.

Server logs are controlled through environment variables. For example:

```json
{
  "rust-glimpser.server.extraEnv": {
    "RUST_GLIMPSER_LOG": "rg_lsp=debug"
  }
}
```
