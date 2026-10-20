/**
 * Shared single-flight pipeline for local, remote, and dialog-based file opens.
 * Registration stays busy through file selection and metadata loading so no
 * other open or query can replace the active file before this operation finishes.
 */
import { useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openFile, openRemoteFile, getFileMetadata, type FileMetadata, type RemoteConnection } from "../lib/tauri";
import { useAppStore } from "../store/appStore";

export function useOpenFile() {
  const openSource = useCallback(async (source?: string | RemoteConnection) => {
    const {
      isLoading, registrationStatus, setRegistrationStatus, setRegistrationError,
      setFile, setFileMetadata, setLastRemoteConnection, setToast,
    } = useAppStore.getState();
    if (isLoading || registrationStatus === "registering") return;

    // Read and claim the shared state before the first await, across hook instances.
    setRegistrationStatus("registering");
    try {
      if (source === undefined) {
        const path = await open({
          filters: [{ name: "Parquet", extensions: ["parquet"] }],
        });
        if (typeof path !== "string" || !path) {
          setRegistrationStatus(registrationStatus);
          return;
        }
        source = path;
      }

      const res = typeof source === "string"
        ? await openFile(source)
        : await openRemoteFile(source);
      if (typeof source !== "string") setLastRemoteConnection(source);
      const displayPath = typeof source === "string"
        ? source
        : source.object_key.split("/").pop() ?? source.object_key;

      // Metadata failure is non-fatal, but keep registration busy until it settles.
      let metadata: FileMetadata | null = null;
      try {
        metadata = await getFileMetadata();
      } catch (err) {
        console.warn("[useOpenFile] getFileMetadata failed (non-fatal):", err);
      }
      setFile(displayPath, res.schema);
      setFileMetadata(metadata);
      if (res.warning) setToast(res.warning);
    } catch (err) {
      // Never log the source: remote connection parameters contain credentials.
      const message = err instanceof Error ? err.message : String(err);
      console.error("[useOpenFile] open failed:", message);
      setRegistrationError(message);
      setRegistrationStatus("error");
    }
  }, []);

  const openPath = useCallback((path: string) => openSource(path), [openSource]);
  const openRemote = useCallback((conn: RemoteConnection) => openSource(conn), [openSource]);
  const openViaDialog = useCallback(() => openSource(), [openSource]);

  return { openPath, openViaDialog, openRemote };
}
