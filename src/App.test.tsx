// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import { useAppStore } from "./store/appStore";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({ onDragDropEvent: async () => () => {} }),
}));
vi.mock("./components/SqlEditor", () => ({ SqlEditor: () => null }));
vi.mock("./components/ResultsGrid", () => ({ ResultsGrid: () => null }));

const credentials = {
  endpoint: "https://active.example.com", access_key_id: "active-key", secret_access_key: "active-secret",
};
const connection = { ...credentials, bucket: "analytics", object_key: "raw/current.parquet" };
const sibling = { key: "raw/ 한글 %#?.parquet ", size: 42, last_modified: null };
let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.resetAllMocks();
  useAppStore.getState().reset();
  useAppStore.getState().setFile("current.parquet", [], connection);
  useAppStore.getState().setLastRemoteConnection({ ...connection, endpoint: "https://saved.example.com" });
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "list_remote_buckets") return { buckets: ["analytics"], next_token: null };
    if (command === "list_remote_prefix") return { prefixes: [], objects: [sibling], next_token: null };
    if (command === "open_remote_file") return { schema: [] };
    if (command === "get_file_metadata") return { total_rows: 1, row_groups: [] };
    throw new Error(`Unexpected command: ${command}`);
  });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
});

async function render() {
  await act(async () => root.render(<App />));
}

function button(text: string) {
  const element = [...container.querySelectorAll<HTMLButtonElement>("button")]
    .find((candidate) => candidate.textContent?.trim() === text);
  if (!element) throw new Error(`Missing button: ${text}`);
  return element;
}

async function click(element: HTMLElement) {
  await act(async () => element.click());
}

it("browses the active file's parent with its credentials and preserves the dataset on cancel", async () => {
  useAppStore.getState().setQueryText("select count(*) from data");
  const before = useAppStore.getState();
  await render();
  const browse = button("Browse");
  browse.focus();
  await click(browse);
  expect(container.querySelector("#remote-endpoint")).toBeNull();
  expect(invoke).toHaveBeenCalledWith("list_remote_buckets", { credentials, pageToken: null });
  expect(invoke).toHaveBeenCalledWith("list_remote_prefix", {
    credentials, bucket: "analytics", prefix: "raw/", pageToken: null,
  });
  expect(useAppStore.getState()).toBe(before);
  await act(async () => container.querySelector('[role="dialog"]')!.dispatchEvent(
    new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
  ));
  expect(container.querySelector('[role="dialog"]')).toBeNull();
  expect(document.activeElement).toBe(browse);
  expect(useAppStore.getState()).toBe(before);
});

it("opens the selected sibling with the same credentials and starts Browse afresh on reopen", async () => {
  await render();
  await click(button("Browse"));
  await click(button(sibling.key.slice(4).trim()));
  await click(button("Open selected file"));
  expect(invoke).toHaveBeenCalledWith("open_remote_file", { conn: { ...connection, object_key: sibling.key } });
  expect(container.querySelector('[role="dialog"]')).toBeNull();
  expect(useAppStore.getState().activeRemoteConnection).toEqual({ ...connection, object_key: sibling.key });
  vi.mocked(invoke).mockClear();
  await click(button("Browse"));
  expect(button("Open selected file").disabled).toBe(true);
  expect(invoke).toHaveBeenCalledWith("list_remote_prefix", {
    credentials, bucket: "analytics", prefix: "raw/", pageToken: null,
  });
});

it("keeps Open Remote on the connection form after closing Browse", async () => {
  await render();
  await click(button("Browse"));
  await click(container.querySelector<HTMLButtonElement>('[aria-label="Close remote browser"]')!);
  vi.mocked(invoke).mockClear();
  await click(button("Open Remote"));
  expect(container.querySelector<HTMLInputElement>("#remote-endpoint")?.value).toBe("https://saved.example.com");
  expect(invoke).not.toHaveBeenCalled();
});

it("disables Browse for a local file even when saved remote credentials exist", async () => {
  useAppStore.getState().setFile("local.parquet", []);
  await render();
  expect(button("Browse").disabled).toBe(true);
  await click(button("Browse"));
  expect(container.querySelector('[role="dialog"]')).toBeNull();
  expect(invoke).not.toHaveBeenCalled();
});

it.each(["query", "registration"] as const)("disables Browse during %s", async (operation) => {
  if (operation === "query") useAppStore.getState().setLoading(true);
  else useAppStore.getState().setRegistrationStatus("registering");
  await render();
  expect(button("Browse").disabled).toBe(true);
  await click(button("Browse"));
  expect(invoke).not.toHaveBeenCalled();
});
