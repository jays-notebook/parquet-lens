import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { tableFromArrays } from "apache-arrow";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { open } from "@tauri-apps/plugin-dialog";
import { getFileMetadata, openFile, openRemoteFile, runQuery, type RemoteConnection } from "../lib/tauri";
import { useAppStore } from "../store/appStore";
import { useOpenFile } from "./useOpenFile";
import { useRunQuery } from "./useRunQuery";

// Keep the real store/actions, but read its live snapshot in the server harness.
// Zustand's normal server subscription always reads the initial application state.
vi.mock("../store/appStore", async (importOriginal) => {
  const { useAppStore } = await importOriginal<typeof import("../store/appStore")>();
  return {
    useAppStore: Object.assign(
      (selector: (state: ReturnType<typeof useAppStore.getState>) => unknown) => selector(useAppStore.getState()),
      useAppStore
    ),
  };
});

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("../lib/tauri", () => ({
  openFile: vi.fn(),
  openRemoteFile: vi.fn(),
  getFileMetadata: vi.fn(),
  runQuery: vi.fn(),
}));

const conn: RemoteConnection = {
  endpoint: "https://example.com", bucket: "bucket", object_key: "remote.parquet",
  access_key_id: "key", secret_access_key: "secret",
};
const schema = [{ name: "id", arrow_type: "Int32", nullable: false }];
const metadata = { total_rows: 1, row_groups: [] };
const result = { table: tableFromArrays({ id: [1] }), rows: [{ id: 1 }], schema, total_rows: 1, capped: false };

// Server rendering runs the real hooks without adding a DOM test dependency.
// Keep these callbacks across store updates to catch stale render snapshots.
function handlers() {
  let value!: ReturnType<typeof useOpenFile> & ReturnType<typeof useRunQuery>;
  function Harness() {
    value = { ...useOpenFile(), ...useRunQuery() };
    return null;
  }
  renderToStaticMarkup(createElement(Harness));
  return value;
}

beforeEach(() => {
  vi.resetAllMocks();
  useAppStore.getState().reset();
  useAppStore.getState().setLastRemoteConnection(null);
  vi.mocked(open).mockResolvedValue("dialog.parquet");
  vi.mocked(openFile).mockResolvedValue({ schema });
  vi.mocked(openRemoteFile).mockResolvedValue({ schema });
  vi.mocked(getFileMetadata).mockResolvedValue(metadata);
  vi.mocked(runQuery).mockResolvedValue(result);
});

afterEach(() => vi.restoreAllMocks());

it("remembers a remote connection only after a successful open", async () => {
  const previous = { ...conn, endpoint: "https://previous.example.com" };
  useAppStore.getState().setLastRemoteConnection(previous);
  let release!: () => void;
  vi.mocked(openRemoteFile).mockImplementationOnce(async () => {
    await new Promise<void>((resolve) => { release = resolve; });
    return { schema };
  });
  const pending = handlers().openRemote(conn);
  try {
    expect(useAppStore.getState().lastRemoteConnection).toEqual(previous);
  } finally {
    release();
    await pending;
  }
  expect(useAppStore.getState().lastRemoteConnection).toEqual(conn);
});

it("keeps the last successful connection when another remote open fails", async () => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  const previous = { ...conn, endpoint: "https://previous.example.com" };
  useAppStore.getState().setLastRemoteConnection(previous);
  vi.mocked(openRemoteFile).mockRejectedValueOnce(new Error("Access denied"));
  await handlers().openRemote(conn);
  expect(useAppStore.getState().lastRemoteConnection).toEqual(previous);
  expect(useAppStore.getState().registrationStatus).toBe("error");
});

it("keeps a successful open usable and reports a credential storage warning", async () => {
  const warning = "Connected, but the connection could not be saved securely.";
  vi.mocked(openRemoteFile).mockResolvedValueOnce({ schema, warning });
  await handlers().openRemote(conn);
  expect(useAppStore.getState().registrationStatus).toBe("registered");
  expect(useAppStore.getState().filePath).toBe("remote.parquet");
  expect(useAppStore.getState().lastRemoteConnection).toEqual(conn);
  expect(useAppStore.getState().toastMessage).toBe(warning);
});

it.each(["local", "remote", "dialog", "metadata", "query"] as const)(
  "rejects new opens and queries while %s is pending, then accepts work again",
  async (operation) => {
    useAppStore.getState().setFile("previous.parquet", schema);
    const first = handlers();
    const second = handlers();
    let release!: () => void;
    const pending = new Promise<void>((resolve) => { release = resolve; });
    if (operation === "local") vi.mocked(openFile).mockImplementationOnce(async () => { await pending; return { schema }; });
    if (operation === "remote") vi.mocked(openRemoteFile).mockImplementationOnce(async () => { await pending; return { schema }; });
    if (operation === "dialog") vi.mocked(open).mockImplementationOnce(async () => { await pending; return "dialog.parquet"; });
    if (operation === "metadata") vi.mocked(getFileMetadata).mockImplementationOnce(async () => { await pending; return metadata; });
    if (operation === "query") vi.mocked(runQuery).mockImplementationOnce(async () => { await pending; return result; });

    const active = operation === "query" ? first.runQueryHandler()
      : operation === "remote" ? first.openRemote(conn)
      : operation === "dialog" ? first.openViaDialog()
      : first.openPath("first.parquet");
    // Let an immediate registration response advance to the pending metadata read.
    await Promise.resolve();
    await Promise.resolve();
    const requests = [open, openFile, openRemoteFile, runQuery].map((fn) => vi.mocked(fn).mock.calls.length);
    const rememberedConnection = useAppStore.getState().lastRemoteConnection;
    const rejected = [second.openPath("ignored.parquet"), second.openRemote(conn), second.openViaDialog(), second.runQueryHandler()];
    try {
      expect([open, openFile, openRemoteFile, runQuery].map((fn) => vi.mocked(fn).mock.calls.length)).toEqual(requests);
      expect(useAppStore.getState().lastRemoteConnection).toBe(rememberedConnection);
    } finally {
      release();
      await Promise.all([active, ...rejected]);
    }

    expect(useAppStore.getState().registrationStatus).toBe("registered");
    expect(useAppStore.getState().isLoading).toBe(false);
    expect(useAppStore.getState().filePath).toBe(operation === "query" ? "previous.parquet"
      : operation === "remote" ? "remote.parquet" : operation === "dialog" ? "dialog.parquet" : "first.parquet");
    await second.openPath("next.parquet");
    expect(useAppStore.getState().filePath).toBe("next.parquet");
    expect(useAppStore.getState().fileMetadata).toEqual(metadata);
  }
);

it.each([false, true])("restores the previous state after dialog cancellation (has file: %s)", async (hasFile) => {
  if (hasFile) useAppStore.getState().setFile("previous.parquet", schema);
  const before = useAppStore.getState();
  vi.mocked(open).mockResolvedValueOnce(null);
  const actions = handlers();
  await actions.openViaDialog();
  expect(useAppStore.getState()).toEqual(before);
  expect(openFile).not.toHaveBeenCalled();
  await actions.openPath("next.parquet");
  expect(useAppStore.getState().filePath).toBe("next.parquet");
});

it.each(["local", "remote", "dialog", "metadata", "query"] as const)("accepts another open after a %s failure", async (operation) => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  vi.spyOn(console, "warn").mockImplementation(() => {});
  useAppStore.getState().setFile("previous.parquet", schema);
  const actions = handlers();
  const failure = new Error("Operation failed");
  if (operation === "query") {
    vi.mocked(runQuery).mockRejectedValueOnce(failure);
    await actions.runQueryHandler();
    expect(useAppStore.getState().queryError).toBe("Operation failed");
  } else {
    if (operation === "local") vi.mocked(openFile).mockRejectedValueOnce(failure);
    if (operation === "remote") vi.mocked(openRemoteFile).mockRejectedValueOnce(failure);
    if (operation === "dialog") vi.mocked(open).mockRejectedValueOnce(failure);
    if (operation === "metadata") vi.mocked(getFileMetadata).mockRejectedValueOnce(failure);
    await (operation === "remote" ? actions.openRemote(conn) : operation === "dialog" ? actions.openViaDialog() : actions.openPath("first.parquet"));
    expect(useAppStore.getState().filePath).toBe(operation === "metadata" ? "first.parquet" : "previous.parquet");
    expect(useAppStore.getState().registrationStatus).toBe(operation === "metadata" ? "registered" : "error");
  }
  await actions.openPath("next.parquet");
  expect(useAppStore.getState().filePath).toBe("next.parquet");
  expect(useAppStore.getState().registrationStatus).toBe("registered");
  expect(useAppStore.getState().isLoading).toBe(false);
});
