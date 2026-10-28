import { useEffect, useRef, useState } from "react";
import { Eye, EyeOff, X } from "lucide-react";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { RemoteBrowser, parentPrefix } from "./RemoteBrowser";
import { useAppStore } from "../store/appStore";
import { getLastRemoteConnection, type RemoteConnection, type RemoteCredentials } from "../lib/tauri";
import "./remote-browser.css";

interface RemoteConnectionModalProps {
  open: boolean;
  onClose: () => void;
  onSubmit: (conn: RemoteConnection) => void | Promise<void>;
  browseConnection?: RemoteConnection | null;
}

const emptyConnection: RemoteConnection = {
  endpoint: "", access_key_id: "", secret_access_key: "", bucket: "", object_key: "",
};

export function RemoteConnectionModal({ open, onClose, onSubmit, browseConnection }: RemoteConnectionModalProps) {
  const [draft, setDraft] = useState<RemoteConnection>(emptyConnection);
  const [credentials, setCredentials] = useState<RemoteCredentials | null>(null);
  const [step, setStep] = useState<"connection" | "browse" | "direct">("connection");
  const [restoring, setRestoring] = useState(false);
  const [revealed, setRevealed] = useState(false);
  const dialogRef = useRef<HTMLDivElement>(null);
  const endpointRef = useRef<HTMLInputElement>(null);
  const primaryRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setStep(browseConnection ? "browse" : "connection");
    setCredentials(browseConnection ? {
      endpoint: browseConnection.endpoint,
      access_key_id: browseConnection.access_key_id,
      secret_access_key: browseConnection.secret_access_key,
    } : null);
    setRevealed(false);
    const previous = browseConnection ?? useAppStore.getState().lastRemoteConnection;
    setDraft(previous ?? emptyConnection);
    setRestoring(!previous);
    if (!previous) {
      void getLastRemoteConnection().then((connection) => {
        if (cancelled) return;
        useAppStore.getState().setLastRemoteConnection(connection);
        setDraft(connection ?? emptyConnection);
      }).catch(() => {
        if (!cancelled) useAppStore.getState().setToast("Could not restore the saved remote connection. Enter connection details manually.");
      }).finally(() => {
        if (!cancelled) setRestoring(false);
      });
    }
    return () => {
      cancelled = true;
      previousFocus?.focus();
    };
  }, [open, browseConnection]);

  useEffect(() => {
    if (!open) return;
    if (restoring) dialogRef.current?.focus();
    else if (step === "connection") {
      if (!endpointRef.current?.value) endpointRef.current?.focus();
      else primaryRef.current?.focus();
    } else {
      dialogRef.current?.querySelector<HTMLInputElement>("input")?.focus();
    }
  }, [open, restoring, step]);

  const canConnect = !!(draft.endpoint.trim() && draft.access_key_id.trim() && draft.secret_access_key.trim());
  function begin(nextStep: "browse" | "direct") {
    if (restoring || !canConnect) return;
    setCredentials({
      endpoint: draft.endpoint.trim(), access_key_id: draft.access_key_id.trim(), secret_access_key: draft.secret_access_key,
    });
    setRevealed(false);
    setStep(nextStep);
  }

  function openObject(bucket: string, key: string) {
    if (!credentials || !bucket.trim() || !key.trim()) return;
    // Close before registration so there is only one blocking overlay.
    onClose();
    void onSubmit({ ...credentials, bucket: bucket.trim(), object_key: key });
  }

  function handleKeyDown(event: React.KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose();
    }
    if (event.key !== "Tab") return;
    const elements = [...(dialogRef.current?.querySelectorAll<HTMLElement>(
      'button:not(:disabled), input:not(:disabled), [tabindex="0"]'
    ) ?? [])].filter((element) => !element.closest("fieldset:disabled"));
    const first = elements[0];
    const last = elements[elements.length - 1];
    if (!first) { event.preventDefault(); dialogRef.current?.focus(); }
    else if (event.shiftKey && (document.activeElement === first || document.activeElement === dialogRef.current)) {
      event.preventDefault(); last.focus();
    } else if (!event.shiftKey && (document.activeElement === last || document.activeElement === dialogRef.current)) {
      event.preventDefault(); first.focus();
    }
  }

  if (!open) return null;

  return (
    <div className="remote-backdrop" onKeyDown={handleKeyDown} onClick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}>
      <div ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby="remote-modal-title" tabIndex={-1}
        className={`remote-dialog ${step === "browse" ? "remote-dialog-wide" : ""}`}>
        <header className="remote-heading">
          <div>
            <h2 id="remote-modal-title">Open Remote Parquet File</h2>
            <p>{step === "connection" ? "Connect to S3 or MinIO, then browse for a file." : credentials?.endpoint}</p>
          </div>
          <Button type="button" variant="ghost" size="icon" aria-label="Close remote browser" onClick={onClose}><X size={18} /></Button>
        </header>
        {step === "connection" ? (
          <form onSubmit={(event) => { event.preventDefault(); begin("browse"); }} aria-busy={restoring}>
            <fieldset className="remote-connection-fields" disabled={restoring}>
              <div className="remote-field">
                <label htmlFor="remote-endpoint">Endpoint URL</label>
                <Input id="remote-endpoint" ref={endpointRef} placeholder="https://storage.example.com" value={draft.endpoint}
                  onChange={(e) => setDraft({ ...draft, endpoint: e.target.value })} />
              </div>
              <div className="remote-field">
                <label htmlFor="remote-access-key-id">Access Key ID</label>
                <Input id="remote-access-key-id" value={draft.access_key_id} autoComplete="off"
                  onChange={(e) => setDraft({ ...draft, access_key_id: e.target.value })} />
              </div>
              <div className="remote-field">
                <label htmlFor="remote-secret-key">Secret Key</label>
                <div className="remote-secret">
                  <Input id="remote-secret-key" type={revealed ? "text" : "password"} value={draft.secret_access_key} autoComplete="off"
                    onChange={(e) => setDraft({ ...draft, secret_access_key: e.target.value })} />
                  <Button type="button" variant="ghost" size="icon" onClick={() => setRevealed(!revealed)} aria-label={revealed ? "Hide secret key" : "Show secret key"}>
                    {revealed ? <EyeOff size={16} /> : <Eye size={16} />}
                  </Button>
                </div>
              </div>
              <Button ref={primaryRef} className="remote-primary" type="submit" disabled={!canConnect}>{restoring ? "Restoring connection…" : "Browse storage"}</Button>
              <Button type="button" variant="outline" disabled={!canConnect} onClick={() => begin("direct")}>Enter exact path</Button>
            </fieldset>
          </form>
        ) : <>
          <div className="remote-row">
            <Button type="button" variant="outline" onClick={() => setStep("connection")}>Change connection</Button>
            {step === "direct" && <Button type="button" variant="outline" onClick={() => setStep("browse")}>Browse storage</Button>}
          </div>
          {step === "browse" && credentials ? (
            <RemoteBrowser credentials={credentials} initialBucket={draft.bucket} initialPrefix={parentPrefix(draft.object_key)}
              onOpen={openObject} onDirect={(bucket, prefix) => {
                setDraft((old) => ({ ...old, bucket, object_key: old.bucket === bucket && parentPrefix(old.object_key) === prefix ? old.object_key : prefix }));
                setStep("direct");
              }} />
          ) : (
            <form className="remote-connection-fields" onSubmit={(event) => { event.preventDefault(); openObject(draft.bucket, draft.object_key); }}>
              <p className="remote-hint">Open a known object even when listing is unavailable.</p>
              <div className="remote-field">
                <label htmlFor="remote-bucket">Bucket</label>
                <Input id="remote-bucket" value={draft.bucket} onChange={(e) => setDraft({ ...draft, bucket: e.target.value })} />
              </div>
              <div className="remote-field">
                <label htmlFor="remote-object-key">Object Key</label>
                <Input id="remote-object-key" value={draft.object_key} onChange={(e) => setDraft({ ...draft, object_key: e.target.value })} />
              </div>
              <Button type="submit" className="remote-primary" disabled={!draft.bucket.trim() || !draft.object_key.trim()}>Open Remote File</Button>
            </form>
          )}
        </>}
      </div>
    </div>
  );
}
