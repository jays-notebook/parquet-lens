import { useEffect, useRef, useState } from "react";
import { ChevronDown, ChevronRight, File, Folder } from "lucide-react";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { formatBytes } from "../lib/format";
import {
  listRemoteBuckets, listRemotePrefix,
  type BucketPage, type PrefixPage, type RemoteCredentials,
} from "../lib/tauri";

export function parentPrefix(key: string): string {
  return key.slice(0, key.lastIndexOf("/") + 1);
}

function ancestors(prefix: string): string[] {
  const result = [""];
  for (let index = prefix.indexOf("/"); index !== -1; index = prefix.indexOf("/", index + 1)) {
    result.push(prefix.slice(0, index + 1));
  }
  return result;
}

interface Listing<T> {
  page: T | null;
  loading: boolean;
  error: string | null;
}

interface RemoteBrowserProps {
  credentials: RemoteCredentials;
  initialBucket: string;
  initialPrefix: string;
  onOpen: (bucket: string, key: string) => void;
  onDirect: (bucket: string, prefix: string) => void;
}

export function RemoteBrowser({ credentials, initialBucket, initialPrefix, onOpen, onDirect }: RemoteBrowserProps) {
  const [buckets, setBuckets] = useState<Listing<BucketPage>>({ page: null, loading: false, error: null });
  const [search, setSearch] = useState("");
  const [bucketInput, setBucketInput] = useState(initialBucket);
  const [bucket, setBucket] = useState(initialBucket);
  const [prefix, setPrefix] = useState(initialPrefix);
  const [pathInput, setPathInput] = useState(initialPrefix);
  const [nodes, setNodes] = useState<Record<string, Listing<PrefixPage>>>({});
  const [expanded, setExpanded] = useState(() => new Set(ancestors(initialPrefix)));
  const [focused, setFocused] = useState("");
  const [selected, setSelected] = useState("");
  const generation = useRef(0);
  const connectionGeneration = useRef(0);
  const treeRef = useRef<HTMLDivElement>(null);
  const pending = useRef(new Set<string>());
  const bucketPending = useRef(false);

  async function loadBuckets(token: string | null = null) {
    if (bucketPending.current) return;
    bucketPending.current = true;
    const requestGeneration = connectionGeneration.current;
    setBuckets((old) => ({ ...old, loading: true, error: null }));
    try {
      const page = await listRemoteBuckets(credentials, token);
      if (requestGeneration !== connectionGeneration.current) return;
      setBuckets((old) => ({
        page: { ...page, buckets: [...new Set([...(token ? old.page?.buckets ?? [] : []), ...page.buckets])] },
        loading: false, error: null,
      }));
    } catch (error) {
      if (requestGeneration === connectionGeneration.current) {
        setBuckets((old) => ({ ...old, loading: false, error: String(error) }));
      }
    } finally {
      if (requestGeneration === connectionGeneration.current) bucketPending.current = false;
    }
  }

  async function loadPrefix(targetBucket: string, target: string, token: string | null = null) {
    if (!targetBucket || pending.current.has(target)) return;
    pending.current.add(target);
    const requestGeneration = generation.current;
    setNodes((old) => ({ ...old, [target]: { page: token ? old[target]?.page ?? null : null, loading: true, error: null } }));
    try {
      const page = await listRemotePrefix(credentials, targetBucket, target, token);
      if (requestGeneration !== generation.current) return;
      setNodes((old) => {
        const previous = token ? old[target]?.page : null;
        return {
          ...old,
          [target]: {
            page: {
              prefixes: [...new Set([...(previous?.prefixes ?? []), ...page.prefixes])],
              objects: [...new Map([...(previous?.objects ?? []), ...page.objects].map((object) => [object.key, object])).values()],
              next_token: page.next_token,
            },
            loading: false, error: null,
          },
        };
      });
    } catch (error) {
      if (requestGeneration === generation.current) {
        setNodes((old) => ({ ...old, [target]: { ...old[target], loading: false, error: String(error) } }));
      }
    } finally {
      if (requestGeneration === generation.current) pending.current.delete(target);
    }
  }

  useEffect(() => {
    void loadBuckets();
    if (initialBucket) void loadPrefix(initialBucket, initialPrefix);
    return () => {
      generation.current++;
      connectionGeneration.current++;
      pending.current.clear();
      bucketPending.current = false;
    };
    // A new connection mounts a fresh browser; credentials never change in place.
  }, []);

  function chooseBucket(name: string) {
    const value = name.trim();
    if (!value) return;
    // Bucket enumeration is connection-scoped; only folder requests change here.
    generation.current++;
    pending.current.clear();
    setBucket(value);
    setBucketInput(value);
    setNodes({});
    setExpanded(new Set([""]));
    setFocused("");
    setSelected("");
    setPrefix("");
    setPathInput("");
    void loadPrefix(value, "");
  }

  function navigate(target: string) {
    setPrefix(target);
    setPathInput(target);
    setSelected("");
    setFocused(target);
    setExpanded((old) => new Set([...old, ...ancestors(target)]));
    if (!nodes[target]?.page && !nodes[target]?.loading) void loadPrefix(bucket, target);
  }

  const current = nodes[prefix];
  const route = ancestors(prefix);
  const visible: { path: string; depth: number }[] = [];
  function walk(path: string, depth: number) {
    visible.push({ path, depth });
    if (!expanded.has(path)) return;
    const routeIndex = route.indexOf(path);
    const routeChild = routeIndex >= 0 ? route[routeIndex + 1] : undefined;
    const children = new Set(nodes[path]?.page?.prefixes ?? []);
    if (routeChild) children.add(routeChild);
    for (const child of [...children].sort()) {
      // Keep malformed server responses from creating a cyclic tree.
      if (child !== path && child.startsWith(path) && child.endsWith("/")) walk(child, depth + 1);
    }
  }
  if (bucket) walk("", 0);

  function focusPath(path: string) {
    setFocused(path);
    const item = [...(treeRef.current?.querySelectorAll<HTMLElement>('[role="treeitem"]') ?? [])]
      .find((node) => node.dataset.path === path);
    item?.focus();
  }

  function toggle(path: string) {
    if (expanded.has(path) && focused !== path && focused.startsWith(path)) focusPath(path);
    setExpanded((old) => {
      const next = new Set(old);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
    if (!expanded.has(path) && !nodes[path]?.page) void loadPrefix(bucket, path);
  }

  function treeKey(event: React.KeyboardEvent, path: string, index: number) {
    switch (event.key) {
      case "ArrowDown": focusPath(visible[Math.min(index + 1, visible.length - 1)].path); break;
      case "ArrowUp": focusPath(visible[Math.max(index - 1, 0)].path); break;
      case "Home": focusPath(""); break;
      case "End": focusPath(visible[visible.length - 1].path); break;
      case "ArrowRight":
        if (!expanded.has(path)) toggle(path);
        else if (visible[index + 1]?.depth > visible[index].depth) focusPath(visible[index + 1].path);
        break;
      case "ArrowLeft":
        if (expanded.has(path)) toggle(path);
        else focusPath(parentPrefix(path.slice(0, -1)));
        break;
      case "Enter": case " ": navigate(path); break;
      default: return;
    }
    event.preventDefault();
  }

  const matches = buckets.page?.buckets.filter((name) => name.toLowerCase().includes(search.toLowerCase())).sort() ?? [];

  return (
    <div className="remote-browser">
      <section className="remote-buckets" aria-label="Buckets">
        <div className="remote-field">
          <label htmlFor="bucket-search">Search buckets</label>
          <Input id="bucket-search" value={search} onChange={(e) => setSearch(e.target.value)} placeholder="Filter loaded buckets…" />
        </div>
        <div className="remote-bucket-list" aria-busy={buckets.loading}>
          {matches.map((name) => (
            <Button key={name} type="button" variant={bucket === name ? "secondary" : "ghost"} onClick={() => chooseBucket(name)}>{name}</Button>
          ))}
        </div>
        {buckets.loading && <p role="status">Loading buckets…</p>}
        {buckets.error && <p role="alert" className="remote-error">{buckets.error}</p>}
        {!buckets.loading && !buckets.error && matches.length === 0 && <p>No matching loaded buckets. You can enter a bucket directly.</p>}
        {buckets.page?.next_token && <p>Search covers loaded buckets. More buckets are available.</p>}
        {(buckets.page?.next_token || buckets.error) && (
          <Button type="button" variant="outline" disabled={buckets.loading}
            onClick={() => void loadBuckets(buckets.page?.next_token ?? null)}>
            {buckets.page?.next_token ? "Load more buckets" : "Retry buckets"}
          </Button>
        )}
        <form className="remote-row" onSubmit={(e) => { e.preventDefault(); chooseBucket(bucketInput); }}>
          <div className="remote-field">
            <label htmlFor="bucket-name">Bucket name</label>
            <Input id="bucket-name" value={bucketInput} onChange={(e) => setBucketInput(e.target.value)} placeholder="Select above or enter a bucket" />
          </div>
          <Button type="submit" variant="outline" disabled={!bucketInput.trim()}>Browse bucket</Button>
        </form>
      </section>

      {bucket && <>
        <nav className="remote-breadcrumbs" aria-label="Object path">
          {route.map((path, index) => (
            <span key={path}>
              {index > 0 && <span aria-hidden="true"> / </span>}
              <button type="button" onClick={() => navigate(path)} aria-current={path === prefix ? "location" : undefined}>
                {index === 0 ? bucket : path.slice(route[index - 1].length, -1) || "/"}
              </button>
            </span>
          ))}
        </nav>
        <form className="remote-row" onSubmit={(e) => { e.preventDefault(); navigate(pathInput && !pathInput.endsWith("/") ? `${pathInput}/` : pathInput); }}>
          <Button type="button" variant="outline" disabled={!prefix} onClick={() => navigate(parentPrefix(prefix.slice(0, -1)))}>Up</Button>
          <div className="remote-field">
            <label htmlFor="folder-prefix">Folder prefix</label>
            <Input id="folder-prefix" value={pathInput} onChange={(e) => setPathInput(e.target.value)} placeholder="Root, or an allowed prefix/" />
          </div>
          <Button type="submit" variant="outline">Go</Button>
        </form>
        <div className="remote-explorer">
          <div className="remote-tree" role="tree" aria-label="Folders" ref={treeRef}>
            {visible.map(({ path, depth }, index) => (
              <div key={path} role="none">
                <div role="treeitem" tabIndex={focused === path ? 0 : -1} data-path={path}
                  aria-level={depth + 1} aria-selected={prefix === path} aria-expanded={expanded.has(path)}
                  style={{ paddingLeft: 8 + depth * 16 }} title={path || bucket}
                  onFocus={() => setFocused(path)} onClick={() => navigate(path)}
                  onKeyDown={(e) => treeKey(e, path, index)}>
                  <span aria-hidden="true" onClick={(e) => { e.stopPropagation(); toggle(path); }}>
                    {expanded.has(path) ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                  </span>
                  <Folder size={14} aria-hidden="true" />
                  <span className="remote-name">{path ? path.slice(parentPrefix(path.slice(0, -1)).length) : bucket}</span>
                </div>
                {expanded.has(path) && nodes[path]?.loading && <p role="status">Loading folder…</p>}
                {expanded.has(path) && nodes[path]?.error && path !== prefix && <Button type="button" variant="ghost" onClick={() => void loadPrefix(bucket, path)}>Retry folder</Button>}
                {expanded.has(path) && nodes[path]?.page?.next_token && path !== prefix &&
                  <Button type="button" variant="ghost" disabled={nodes[path]?.loading}
                    onClick={() => void loadPrefix(bucket, path, nodes[path].page!.next_token)}>More folders…</Button>}
              </div>
            ))}
          </div>
          <section className="remote-files" aria-label="Files in current folder" aria-busy={current?.loading ?? false}>
            {current?.error && <div className="remote-error" role="alert">
              <p>{current.error}</p>
              <Button type="button" variant="outline" onClick={() => void loadPrefix(bucket, prefix, current.page?.next_token ?? null)}>Retry folder</Button>
            </div>}
            <table>
              <thead><tr><th>Name</th><th>Size</th><th>Modified</th></tr></thead>
              <tbody>
                {current?.page?.objects.map((object) => (
                  <tr key={object.key} data-selected={selected === object.key}>
                    <td><button type="button" title={object.key} aria-pressed={selected === object.key} onClick={() => setSelected(object.key)}>
                      <File size={14} aria-hidden="true" /><span>{object.key.slice(prefix.length)}</span>
                    </button></td>
                    <td>{formatBytes(object.size)}</td>
                    <td>{new Date(object.last_modified).toLocaleString()}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {current?.loading && <p role="status">Loading entries…</p>}
            {!current?.loading && !current?.error && current?.page && current.page.objects.length === 0 &&
              <p>{current.page.prefixes.length ? "No files on this page. Explore a folder in the tree." : current.page.next_token ? "No files on this page. Load more entries." : "This folder is empty."}</p>}
            {current?.page?.next_token && <Button type="button" variant="outline" disabled={current.loading}
              onClick={() => void loadPrefix(bucket, prefix, current.page!.next_token)}>Load more entries</Button>}
          </section>
        </div>
      </>}
      <div className="remote-footer">
        <Button type="button" variant="outline" onClick={() => onDirect(bucket || bucketInput.trim(), prefix)}>Enter exact object key</Button>
        <span className="remote-selection" title={selected}>{selected || "Select a file to open"}</span>
        <Button type="button" className="remote-primary" disabled={!bucket || !selected} onClick={() => onOpen(bucket, selected)}>Open selected file</Button>
      </div>
    </div>
  );
}
