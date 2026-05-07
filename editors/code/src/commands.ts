export const EXTENSION_COMMANDS = {
  restartServer: "rust-glancer.restartServer",
  reindexWorkspace: "rust-glancer.reindexWorkspace",
  goToTypeFromHover: "rust-glancer.gotoTypeFromHover",
  testGetState: "rust-glancer.test.getState",
} as const;

export const SERVER_COMMANDS = {
  reindexWorkspace: "rust-glancer.internal.reindexWorkspace",
} as const;
