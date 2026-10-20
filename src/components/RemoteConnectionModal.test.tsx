// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { RemoteConnectionModal } from "./RemoteConnectionModal";
import { useAppStore } from "../store/appStore";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const saved = {
  endpoint: "http://localhost:9000", access_key_id: "access", secret_access_key: "secret",
  bucket: "analytics", object_key: "raw/previous.parquet",
};
const empty = { prefixes: [], objects: [], next_token: null };
const file = { key: "raw/ 한글 %#?.parquet ", size: 42, last_modified: "2026-10-20T00:00:00Z" };
let container: HTMLDivElement;
let root: Root;
const submit = vi.fn<(connection: typeof saved) => void>();
const close = vi.fn<() => void>();

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.resetAllMocks();
  useAppStore.getState().reset();
  useAppStore.getState().setLastRemoteConnection(null);
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_last_remote_connection") return null;
    if (command === "list_remote_buckets") return { buckets: ["analytics", "logs"], next_token: null };
    return empty;
  });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
});

async function render(open = true) {
  await act(async () => {
    root.render(<RemoteConnectionModal open={open} onClose={close} onSubmit={submit} />);
  });
}

function button(text: string) {
  const result = [...container.querySelectorAll<HTMLButtonElement>("button")]
    .find((b) => b.textContent?.trim() === text);
  if (!result) throw new Error(`Missing button: ${text}`);
  return result;
}

async function click(element: HTMLElement) {
  await act(async () => element.click());
}

async function input(label: string, value: string) {
  const element = [...container.querySelectorAll("input")]
    .find((i) => i.labels?.[0]?.textContent === label);
  if (!element) throw new Error(`Missing input: ${label}`);
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(element, value);
    element.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

async function connect() {
  await input("Endpoint URL", saved.endpoint);
  await input("Access Key ID", saved.access_key_id);
  await input("Secret Key", saved.secret_access_key);
  await click(button("Browse storage"));
}

it("connects without a bucket or key, filters buckets, and opens the exact selected key", async () => {
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "get_last_remote_connection") return null;
    if (command === "list_remote_buckets") return { buckets: ["analytics", "logs"], next_token: null };
    const { prefix } = args as { prefix: string };
    return prefix === "" ? { ...empty, prefixes: ["raw/"] } : { ...empty, objects: [file] };
  });
  await render();
  await connect();
  await input("Search buckets", "ana");
  expect(button("analytics")).toBeTruthy();
  expect(container.textContent).not.toContain("logs");
  await click(button("analytics"));
  const raw = [...container.querySelectorAll<HTMLElement>('[role="treeitem"]')]
    .find((node) => node.textContent?.trim() === "raw/")!;
  await click(raw);
  expect(container.textContent).toContain(file.key.slice(4));
  expect(submit).not.toHaveBeenCalled();
  await click(button(file.key.slice(4).trim()));
  await click(button("Open selected file"));
  expect(submit).toHaveBeenCalledWith({ ...saved, object_key: file.key });
  expect(close.mock.invocationCallOrder[0]).toBeLessThan(submit.mock.invocationCallOrder[0]);
  expect(useAppStore.getState().lastRemoteConnection).toBeNull();
  expect(useAppStore.getState().filePath).toBeNull();
});

it("restores the successful connection and starts at its parent without listing root", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_last_remote_connection") return saved;
    if (command === "list_remote_buckets") return { buckets: [], next_token: null };
    return { ...empty, objects: [file] };
  });
  await render();
  await click(button("Browse storage"));
  expect(invoke).toHaveBeenCalledWith("list_remote_prefix", {
    credentials: { endpoint: saved.endpoint, access_key_id: "access", secret_access_key: "secret" },
    bucket: "analytics", prefix: "raw/", pageToken: null,
  });
  expect(container.textContent).toContain(file.key.slice(4));
});

it("keeps direct opening available when bucket and prefix listing are denied", async () => {
  useAppStore.getState().setLastRemoteConnection(saved);
  vi.mocked(invoke).mockRejectedValue("Listing was denied.");
  await render();
  await click(button("Browse storage"));
  expect(container.textContent).toContain("Listing was denied.");
  await click(button("Enter exact object key"));
  await input("Object Key", " private/file.parquet ");
  await click(button("Open Remote File"));
  expect(submit).toHaveBeenCalledWith({ ...saved, object_key: " private/file.parquet " });
});

it("supports manual buckets, a restricted starting prefix, breadcrumbs, and more folder pages", async () => {
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "get_last_remote_connection") return null;
    if (command === "list_remote_buckets") return { buckets: [], next_token: null };
    const { prefix, pageToken } = args as { prefix: string; pageToken: string | null };
    if (prefix === "") throw "Root listing was denied.";
    return pageToken ? { ...empty, objects: [file] } : { ...empty, prefixes: ["raw/2026/"], next_token: "next" };
  });
  await render();
  await connect();
  await input("Bucket name", "private-bucket");
  await click(button("Browse bucket"));
  expect(container.textContent).toContain("Root listing was denied.");
  await input("Folder prefix", "raw");
  await click(button("Go"));
  expect(container.textContent).toContain("2026/");
  await click(button("Load more entries"));
  expect(container.textContent).toContain(file.key.slice(4));
  expect(container.textContent).toContain("2026/");
  await click(button("Up"));
  expect(container.textContent).toContain("Root listing was denied.");
});

it("does not let a late bucket response replace a newer bucket or preserve its selection", async () => {
  let resolveOld!: (value: unknown) => void;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "get_last_remote_connection") return null;
    if (command === "list_remote_buckets") return { buckets: ["analytics", "logs"], next_token: null };
    const { bucket } = args as { bucket: string };
    if (bucket === "analytics") return new Promise((resolve) => { resolveOld = resolve; });
    return { ...empty, objects: [{ ...file, key: "new.parquet" }] };
  });
  await render();
  await connect();
  await click(button("analytics"));
  await click(button("logs"));
  await act(async () => resolveOld({ ...empty, objects: [{ ...file, key: "old.parquet" }] }));
  expect(container.textContent).toContain("new.parquet");
  expect(container.textContent).not.toContain("old.parquet");
  expect(button("Open selected file").disabled).toBe(true);
});

it("discloses incomplete bucket search results and appends the next page", async () => {
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "get_last_remote_connection") return null;
    if (command === "list_remote_buckets") {
      return (args as { pageToken: string | null }).pageToken
        ? { buckets: ["warehouse"], next_token: null }
        : { buckets: ["analytics"], next_token: "next" };
    }
    return empty;
  });
  await render();
  await connect();
  await input("Search buckets", "warehouse");
  expect(container.textContent).toContain("loaded buckets");
  await click(button("Load more buckets"));
  expect(button("warehouse")).toBeTruthy();
});

it("keeps a pending bucket list when a manually entered bucket is opened", async () => {
  let resolveBuckets!: (value: unknown) => void;
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_last_remote_connection") return null;
    if (command === "list_remote_buckets") return new Promise((resolve) => { resolveBuckets = resolve; });
    return empty;
  });
  await render();
  await connect();
  await input("Bucket name", "private");
  await click(button("Browse bucket"));
  await act(async () => resolveBuckets({ buckets: ["analytics"], next_token: null }));
  expect(button("analytics")).toBeTruthy();
  expect(container.textContent).not.toContain("Loading buckets");
});

it("does not let a late folder response change the visible path or enable opening", async () => {
  let resolveOld!: (value: unknown) => void;
  useAppStore.getState().setLastRemoteConnection(saved);
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "list_remote_buckets") return { buckets: [], next_token: null };
    if ((args as { prefix: string }).prefix === "raw/") return new Promise((resolve) => { resolveOld = resolve; });
    return { ...empty, objects: [{ ...file, key: "new/current.parquet" }] };
  });
  await render();
  await click(button("Browse storage"));
  await input("Folder prefix", "new/");
  await click(button("Go"));
  await act(async () => resolveOld({ ...empty, objects: [file] }));
  expect(container.textContent).toContain("current.parquet");
  expect(container.textContent).not.toContain(file.key);
  expect(button("Open selected file").disabled).toBe(true);
});

it("does not reuse results or file selection after changing credentials", async () => {
  let resolveOld!: (value: unknown) => void;
  useAppStore.getState().setLastRemoteConnection(saved);
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "list_remote_buckets") return { buckets: ["analytics"], next_token: null };
    if ((args as { credentials: { endpoint: string } }).credentials.endpoint === saved.endpoint) {
      return new Promise((resolve) => { resolveOld = resolve; });
    }
    return empty;
  });
  await render();
  await click(button("Browse storage"));
  await click(button("Change connection"));
  await input("Endpoint URL", "https://new.example.com");
  await click(button("Browse storage"));
  await act(async () => resolveOld({ ...empty, objects: [file] }));
  expect(container.textContent).not.toContain(file.key);
  expect(button("Open selected file").disabled).toBe(true);
  expect(useAppStore.getState().lastRemoteConnection).toEqual(saved);
});

it("contains keyboard focus, supports Escape during restore, and ignores a late restore", async () => {
  let resolveRestore!: (value: unknown) => void;
  vi.mocked(invoke).mockImplementation(() => new Promise((resolve) => { resolveRestore = resolve; }));
  await render();
  expect(document.activeElement?.getAttribute("role")).toBe("dialog");
  await act(async () => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(close).toHaveBeenCalledOnce();
  await render(false);
  await act(async () => resolveRestore(saved));
  expect(useAppStore.getState().lastRemoteConnection).toBeNull();
});

it("wraps Tab focus within the dialog and supports arrow navigation in the tree", async () => {
  useAppStore.getState().setLastRemoteConnection({ ...saved, object_key: "root.parquet" });
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "list_remote_buckets") return { buckets: ["analytics"], next_token: null };
    return (args as { prefix: string }).prefix === "" ? { ...empty, prefixes: ["raw/"] } : empty;
  });
  await render();
  const controls = [...container.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled)')];
  controls[controls.length - 1].focus();
  await act(async () => controls[controls.length - 1].dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true })));
  expect(document.activeElement).toBe(controls[0]);
  await click(button("Browse storage"));
  const rootNode = container.querySelector<HTMLElement>('[role="treeitem"]')!;
  rootNode.focus();
  await act(async () => rootNode.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })));
  expect(document.activeElement?.textContent?.trim()).toBe("raw/");
});

it("keeps a visible tree Tab stop when the focused descendant is collapsed", async () => {
  useAppStore.getState().setLastRemoteConnection(saved);
  await render();
  await click(button("Browse storage"));
  const rows = [...container.querySelectorAll<HTMLElement>('[role="treeitem"]')];
  await act(async () => rows[1].focus());
  await click(rows[0].querySelector<HTMLElement>('span[aria-hidden="true"]')!);
  expect(container.querySelector('[role="treeitem"][tabindex="0"]')).toBe(rows[0]);
  expect(document.activeElement).toBe(rows[0]);
});
