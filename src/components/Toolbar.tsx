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
}

export function Toolbar({ onOpenRemote }: ToolbarProps) {
  const filePath = useAppStore((s) => s.filePath);
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
          maxWidth: "calc(100% - 120px)",
        }}
        title={filePath ?? ""}
      >
        {filename}
      </span>

      {/* Right region: Open File + Open Remote ghost buttons (REMOTE-01 / D-02) */}
      <div style={{ display: "flex", gap: "8px", flexShrink: 0 }}>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => void openViaDialog()}
          style={{ color: "var(--foreground)" }}
        >
          Open File
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={onOpenRemote}
          style={{ color: "var(--foreground)" }}
        >
          Open Remote
        </Button>
      </div>
    </div>
  );
}
