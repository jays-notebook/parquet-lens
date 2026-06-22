/**
 * Remote connection modal for opening MinIO/S3-compatible Parquet objects.
 *
 * 5-field centered modal form. Props:
 *   open     — whether the modal is visible
 *   onClose  — called to dismiss the modal (Escape key, backdrop click)
 *   onSubmit — called AFTER onClose() with the filled RemoteConnection (D-03)
 *
 * Key design decisions (from 05-CONTEXT.md / 05-UI-SPEC.md):
 *   D-03: onClose() fires BEFORE onSubmit() so the modal is never visible while
 *         the Phase 4 RegistrationOverlay is active — no stacked overlays.
 *   D-04: Secret key field is type="password" by default; a reveal toggle switches
 *         it to type="text" (Eye / EyeOff icons from lucide-react).
 *   D-05: Form state is initialized from lastRemoteConnection in the Zustand store
 *         so values persist within the same session.
 *   D-06: The modal does NOT render its own error state — all errors route to the
 *         Phase 4 RegistrationOverlay.
 *   D-07: Submit button is disabled while any of the 5 fields is empty.
 *
 * Source: 05-UI-SPEC.md §RemoteConnectionModal, §Layout Contract, §Copywriting Contract,
 *         §Accessibility Contract; 05-PATTERNS.md §RemoteConnectionModal.tsx
 */
import { useState, useEffect, useRef } from "react";
import { Eye, EyeOff } from "lucide-react";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { useAppStore } from "../store/appStore";
import type { RemoteConnection } from "../lib/tauri";

interface RemoteConnectionModalProps {
  open: boolean;
  onClose: () => void;
  onSubmit: (conn: RemoteConnection) => void | Promise<void>;
}

export function RemoteConnectionModal({
  open,
  onClose,
  onSubmit,
}: RemoteConnectionModalProps) {
  // D-05: autofill from last-used connection within the same session
  const lastRemoteConnection = useAppStore((s) => s.lastRemoteConnection);

  // Form state — initialized from lastRemoteConnection so fields re-populate on re-open
  const [endpoint, setEndpoint] = useState(
    lastRemoteConnection?.endpoint ?? ""
  );
  const [bucket, setBucket] = useState(lastRemoteConnection?.bucket ?? "");
  const [objectKey, setObjectKey] = useState(
    lastRemoteConnection?.object_key ?? ""
  );
  const [accessKeyId, setAccessKeyId] = useState(
    lastRemoteConnection?.access_key_id ?? ""
  );
  const [secretAccessKey, setSecretAccessKey] = useState(
    lastRemoteConnection?.secret_access_key ?? ""
  );

  // D-04: secret key reveal toggle
  const [revealed, setRevealed] = useState(false);

  // Refs for focus management on open (accessibility — matches RegistrationOverlay pattern)
  const endpointRef = useRef<HTMLInputElement>(null);
  const bucketRef = useRef<HTMLInputElement>(null);
  const objectKeyRef = useRef<HTMLInputElement>(null);
  const accessKeyIdRef = useRef<HTMLInputElement>(null);
  const secretAccessKeyRef = useRef<HTMLInputElement>(null);
  const submitButtonRef = useRef<HTMLButtonElement>(null);

  // Re-populate form state when lastRemoteConnection changes (e.g. after a successful open)
  // and reset the reveal toggle on each open
  useEffect(() => {
    if (open) {
      setEndpoint(lastRemoteConnection?.endpoint ?? "");
      setBucket(lastRemoteConnection?.bucket ?? "");
      setObjectKey(lastRemoteConnection?.object_key ?? "");
      setAccessKeyId(lastRemoteConnection?.access_key_id ?? "");
      setSecretAccessKey(lastRemoteConnection?.secret_access_key ?? "");
      setRevealed(false);
    }
  }, [open, lastRemoteConnection]);

  // Focus management: first empty field on open, or submit button if all autofilled
  useEffect(() => {
    if (!open) return;

    // Schedule focus after render so refs are attached
    const id = setTimeout(() => {
      const refs = [
        { ref: endpointRef, value: lastRemoteConnection?.endpoint ?? "" },
        { ref: bucketRef, value: lastRemoteConnection?.bucket ?? "" },
        { ref: objectKeyRef, value: lastRemoteConnection?.object_key ?? "" },
        { ref: accessKeyIdRef, value: lastRemoteConnection?.access_key_id ?? "" },
        { ref: secretAccessKeyRef, value: lastRemoteConnection?.secret_access_key ?? "" },
      ];
      const firstEmpty = refs.find((r) => r.value === "");
      if (firstEmpty) {
        firstEmpty.ref.current?.focus();
      } else {
        submitButtonRef.current?.focus();
      }
    }, 0);

    return () => clearTimeout(id);
  }, [open, lastRemoteConnection]);

  // D-07: disable submit while any field is empty
  const allFieldsFilled =
    endpoint.trim() !== "" &&
    bucket.trim() !== "" &&
    objectKey.trim() !== "" &&
    accessKeyId.trim() !== "" &&
    secretAccessKey.trim() !== "";

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!allFieldsFilled) return;

    const conn: RemoteConnection = {
      endpoint: endpoint.trim(),
      bucket: bucket.trim(),
      object_key: objectKey.trim(),
      access_key_id: accessKeyId.trim(),
      secret_access_key: secretAccessKey,
    };

    // D-03: close modal FIRST, then hand off to openRemote so the Phase 4
    // RegistrationOverlay is the only visible overlay during registration.
    onClose();
    void onSubmit(conn);
  }

  // Escape key closes the modal (accessibility)
  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Escape") {
      onClose();
    }
  }

  // Return null when closed — no DOM node, no layout cost
  if (!open) return null;

  return (
    <div
      // Backdrop: fixed inset-0, rgba(0,0,0,0.65), zIndex 70 (above RegistrationOverlay at 60)
      style={{
        position: "fixed",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        backgroundColor: "rgba(0, 0, 0, 0.65)",
        zIndex: 70,
        pointerEvents: "all",
      }}
      onKeyDown={handleKeyDown}
      // Clicking the backdrop closes the modal
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      {/* Modal card: var(--card), 400px wide, 24px padding */}
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="remote-modal-title"
        style={{
          backgroundColor: "var(--card)",
          width: "400px",
          borderRadius: "var(--radius)",
          padding: "24px",
          display: "flex",
          flexDirection: "column",
          gap: "16px",
        }}
      >
        {/* Title: 16px / 600 weight (UI-SPEC.md §Typography heading role) */}
        <span
          id="remote-modal-title"
          style={{
            fontSize: "16px",
            fontWeight: 600,
            color: "var(--foreground)",
            lineHeight: 1.2,
          }}
        >
          Open Remote Parquet File
        </span>

        {/* 5-field form — gap: 16px between field groups handled by card's column flex */}
        <form
          onSubmit={handleSubmit}
          style={{ display: "flex", flexDirection: "column", gap: "12px" }}
        >
          {/* Field 1: Endpoint URL */}
          <div style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
            <label
              htmlFor="remote-endpoint"
              style={{
                fontSize: "14px",
                fontWeight: 400,
                color: "var(--muted-foreground)",
                lineHeight: 1.5,
              }}
            >
              Endpoint URL
            </label>
            <Input
              id="remote-endpoint"
              ref={endpointRef}
              type="text"
              placeholder="http://host:port"
              value={endpoint}
              onChange={(e) => setEndpoint(e.target.value)}
            />
          </div>

          {/* Field 2: Bucket */}
          <div style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
            <label
              htmlFor="remote-bucket"
              style={{
                fontSize: "14px",
                fontWeight: 400,
                color: "var(--muted-foreground)",
                lineHeight: 1.5,
              }}
            >
              Bucket
            </label>
            <Input
              id="remote-bucket"
              ref={bucketRef}
              type="text"
              value={bucket}
              onChange={(e) => setBucket(e.target.value)}
            />
          </div>

          {/* Field 3: Object Key */}
          <div style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
            <label
              htmlFor="remote-object-key"
              style={{
                fontSize: "14px",
                fontWeight: 400,
                color: "var(--muted-foreground)",
                lineHeight: 1.5,
              }}
            >
              Object Key
            </label>
            <Input
              id="remote-object-key"
              ref={objectKeyRef}
              type="text"
              value={objectKey}
              onChange={(e) => setObjectKey(e.target.value)}
            />
          </div>

          {/* Field 4: Access Key ID */}
          <div style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
            <label
              htmlFor="remote-access-key-id"
              style={{
                fontSize: "14px",
                fontWeight: 400,
                color: "var(--muted-foreground)",
                lineHeight: 1.5,
              }}
            >
              Access Key ID
            </label>
            <Input
              id="remote-access-key-id"
              ref={accessKeyIdRef}
              type="text"
              value={accessKeyId}
              onChange={(e) => setAccessKeyId(e.target.value)}
            />
          </div>

          {/* Field 5: Secret Key — masked by default with reveal toggle (D-04) */}
          <div style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
            <label
              htmlFor="remote-secret-key"
              style={{
                fontSize: "14px",
                fontWeight: 400,
                color: "var(--muted-foreground)",
                lineHeight: 1.5,
              }}
            >
              Secret Key
            </label>
            {/* Wrapper: position relative so the toggle button can be absolutely positioned */}
            <div style={{ position: "relative" }}>
              <Input
                id="remote-secret-key"
                ref={secretAccessKeyRef}
                type={revealed ? "text" : "password"}
                value={secretAccessKey}
                onChange={(e) => setSecretAccessKey(e.target.value)}
                // Right padding clears the reveal toggle icon
                style={{ paddingRight: "40px" }}
              />
              {/* Reveal toggle: Eye (masked) / EyeOff (revealed) */}
              <Button
                type="button"
                variant="ghost"
                size="icon"
                onClick={() => setRevealed((v) => !v)}
                aria-label={revealed ? "Hide secret key" : "Show secret key"}
                style={{
                  position: "absolute",
                  right: "8px",
                  top: "50%",
                  transform: "translateY(-50%)",
                }}
              >
                {revealed ? <EyeOff size={16} /> : <Eye size={16} />}
              </Button>
            </div>
          </div>

          {/* Submit: full-width, 44px min-height, disabled while any field empty (D-07) */}
          <Button
            ref={submitButtonRef}
            type="submit"
            variant="default"
            disabled={!allFieldsFilled}
            style={{ width: "100%", minHeight: "44px" }}
          >
            Open Remote File
          </Button>
        </form>
      </div>
    </div>
  );
}
