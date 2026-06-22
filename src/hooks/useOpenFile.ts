/**
 * Shared file-open pipeline (WR-02).
 *
 * Owns the full open/registration lifecycle so the three call sites
 * (drag-drop in App, dialog in OpenScreen, dialog in Toolbar) no longer
 * each keep a divergent copy — the divergence is exactly what produced
 * CR-01. Both `openPath` (path already known) and `openViaDialog` (resolve
 * a path through the OS dialog first) funnel through one implementation.
 *
 * Lifecycle (D-08 / LOAD-01):
 *   setRegistrationStatus("registering")  → overlay appears immediately
 *   openFile(path)                        → backend atomic register (D-04/D-05)
 *   setFile(path, schema)                 → status flips to "registered", overlay dismisses
 *   getFileMetadata()                     → sidebar data (non-fatal; guarded by openSeq, WR-03)
 *   on error → setRegistrationError + status "error"
 */
import { useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openFile, openRemoteFile, getFileMetadata, type RemoteConnection } from "../lib/tauri";
import { useAppStore } from "../store/appStore";

export function useOpenFile() {
  const setFile = useAppStore((s) => s.setFile);
  const setFileMetadata = useAppStore((s) => s.setFileMetadata);
  const setRegistrationStatus = useAppStore((s) => s.setRegistrationStatus);
  const setRegistrationError = useAppStore((s) => s.setRegistrationError);
  const setLastRemoteConnection = useAppStore((s) => s.setLastRemoteConnection);

  /** Open a known path through the full registration lifecycle. */
  const openPath = useCallback(
    async (path: string) => {
      // Phase 4 (LOAD-01): signal registration start BEFORE openFile — overlay appears immediately.
      setRegistrationStatus("registering");
      try {
        const res = await openFile(path);
        // setFile flips registrationStatus to 'registered' atomically (D-08) and
        // bumps openSeq — capture the new token to guard the metadata fetch below.
        setFile(path, res.schema); // D-16: immediate replace, resets editor + results
        const seq = useAppStore.getState().openSeq;
        // Fetch file metadata immediately after open (META-02 / META-03 sidebar data).
        // Non-fatal: fileMetadata stays null and the sidebar header degrades to "Loading…".
        try {
          const meta = await getFileMetadata();
          // WR-03: apply only if no newer open superseded this one. A monotonic
          // token (not a filePath compare) correctly rejects a stale response even
          // when the user re-opens the SAME path while a prior fetch is in flight.
          if (useAppStore.getState().openSeq === seq) {
            setFileMetadata(meta);
          }
        } catch (metaErr) {
          console.warn("[useOpenFile] getFileMetadata failed (non-fatal):", metaErr);
        }
      } catch (err) {
        // Phase 4 (D-06/LOAD-01): route error to the overlay, not a toast/alert.
        console.error("[useOpenFile] open failed:", err);
        setRegistrationError(err instanceof Error ? err.message : String(err));
        setRegistrationStatus("error");
      }
    },
    [setFile, setFileMetadata, setRegistrationStatus, setRegistrationError]
  );

  /**
   * Open a remote Parquet object through the full registration lifecycle.
   *
   * Mirrors `openPath` exactly — routes through the same setRegistrationStatus
   * lifecycle so Phase 4 overlay and Run-gating apply unchanged (anti-divergence,
   * CR-01 prevention). Never calls `invoke` directly: all IPC goes through
   * `openRemoteFile` (the typed wrapper in lib/tauri.ts).
   *
   * D-05: persists autofill values via `setLastRemoteConnection` BEFORE invoking
   * the backend so the form re-populates even if the backend rejects the connection.
   *
   * T-05-06: only `err`/`err.message` is logged on error — never the `conn` object
   * (which contains secret_access_key).
   */
  const openRemote = useCallback(
    async (conn: RemoteConnection) => {
      // D-05: persist autofill values first so re-populating works even on error
      setLastRemoteConnection(conn);
      // Phase 4 (LOAD-01): signal registration start — overlay appears immediately
      setRegistrationStatus("registering");
      try {
        const res = await openRemoteFile(conn);
        // Display: use the object_key leaf segment as the visible filename
        // so Toolbar's basename() renders cleanly (RESEARCH.md §Pitfall 5)
        const displayPath = conn.object_key.split("/").pop() ?? conn.object_key;
        // setFile flips registrationStatus to 'registered' atomically (D-08) and bumps openSeq
        setFile(displayPath, res.schema);
        const seq = useAppStore.getState().openSeq;
        // Fetch file metadata immediately after open (non-fatal, same as openPath)
        try {
          const meta = await getFileMetadata();
          // WR-03: apply only if no newer open superseded this one
          if (useAppStore.getState().openSeq === seq) {
            setFileMetadata(meta);
          }
        } catch (metaErr) {
          console.warn("[useOpenFile] getFileMetadata failed (non-fatal):", metaErr);
        }
      } catch (err) {
        // T-05-06: log only the error message, never the conn object (credentials)
        console.error("[useOpenFile] remote open failed:", err instanceof Error ? err.message : String(err));
        setRegistrationError(err instanceof Error ? err.message : String(err));
        setRegistrationStatus("error");
      }
    },
    [setFile, setFileMetadata, setRegistrationStatus, setRegistrationError, setLastRemoteConnection]
  );

  /** Resolve a path through the OS file dialog, then open it. No-op if cancelled. */
  const openViaDialog = useCallback(async () => {
    let path: string | null;
    try {
      // `open` returns string | null on desktop (null when the user cancels).
      path = await open({
        filters: [{ name: "Parquet", extensions: ["parquet"] }],
      });
    } catch (err) {
      // Dialog itself failed (rare) — surface via the overlay for consistency.
      console.error("[useOpenFile] dialog failed:", err);
      setRegistrationError(err instanceof Error ? err.message : String(err));
      setRegistrationStatus("error");
      return;
    }
    if (typeof path === "string" && path) {
      await openPath(path);
    }
  }, [openPath, setRegistrationError, setRegistrationStatus]);

  return { openPath, openViaDialog, openRemote };
}
