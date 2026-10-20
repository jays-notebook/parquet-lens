/**
 * Screen B — Toolbar shown after a file is loaded.
 *
 * 48px bar with --muted background and --border bottom border.
 * Shows the opened filename and an "Open File" ghost button to re-open.
 *
 * Source: UI-SPEC.md §Screen B Toolbar + §Open Button interaction states
 */
import { useAppStore } from "../store/appStore";
import { useOpenFile } from "../hooks/useOpenFile";
import { Button } from "./ui/button";

/** Extracts the filename from an absolute path (Windows or Unix). */
function basename(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

interface ToolbarProps {
  /** Called when the user clicks the "Open Remote" button (REMOTE-01). */
  onOpenRemote: () => void;
  onBrowseRemote: () => void;
}

export function Toolbar({ onOpenRemote, onBrowseRemote }: ToolbarProps) {
  const filePath = useAppStore((s) => s.filePath);
  const hasRemoteConnection = useAppStore((s) => s.activeRemoteConnection !== null);
  const busy = useAppStore((s) => s.isLoading || s.registrationStatus === "registering");
  // Shared open pipeline (WR-02) — dialog path resolves then registers.
  const { openViaDialog } = useOpenFile();

  const filename = filePath ? basename(filePath) : "";

  return (
    <div
      style={{
        height: "48px",
        backgroundColor: "var(--muted)",
        borderBottom: "1px solid var(--border)",
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "0 16px",
        flexShrink: 0,
      }}
    >
      {/* Filename label: 12px, --muted-foreground (UI-SPEC.md §Typography label role) */}
      <span
        style={{
          fontSize: "12px",
          fontWeight: 400,
          color: "var(--muted-foreground)",
          lineHeight: 1.4,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          minWidth: 0,
          marginRight: "8px",
        }}
        title={filePath ?? ""}
      >
        {filename}
      </span>

      {/* File actions */}
      <div style={{ display: "flex", gap: "8px", flexShrink: 0 }}>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => void openViaDialog()}
          disabled={busy}
          style={{ color: "var(--foreground)" }}
        >
          Open File
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={onOpenRemote}
          disabled={busy}
          style={{ color: "var(--foreground)" }}
        >
          Open Remote
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={onBrowseRemote}
          disabled={busy || !hasRemoteConnection}
          title={hasRemoteConnection ? "Browse using the current remote connection" : "Open a remote file to browse its storage"}
          style={{ color: "var(--foreground)" }}
        >
          Browse
        </Button>
      </div>
    </div>
  );
}
