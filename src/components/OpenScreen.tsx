/**
 * Screen A — Welcome screen shown when no file is loaded.
 *
 * Full-window centered layout with a click-to-open card.
 * The card is the entire click target and opens the OS file dialog
 * filtered to .parquet files.
 *
 * Drag-and-drop is handled at the App root level (D-14 whole-window scope)
 * so it is active on both Screen A and Screen B. OpenScreen only handles
 * the dialog-based open path.
 *
 * Source: UI-SPEC.md §Screen A + §File-Open Card interaction states
 */
import { useOpenFile } from "../hooks/useOpenFile";

interface OpenScreenProps {
  /** Called when the user clicks the secondary "Open remote file" affordance (REMOTE-01). */
  onOpenRemote: () => void;
}

export function OpenScreen({ onOpenRemote }: OpenScreenProps) {
  // Shared open pipeline (WR-02) — dialog path resolves then registers.
  const { openViaDialog } = useOpenFile();

  function handleOpen() {
    void openViaDialog();
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      void handleOpen();
    }
  }

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        height: "100vh",
        width: "100vw",
        backgroundColor: "var(--background)",
      }}
    >
      {/* Column container: card + secondary affordance stacked with gap:16px (UI-SPEC.md §OpenScreen) */}
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: "16px",
        }}
      >
        {/*
         * The card: 480x200, 2px dashed --primary border, --card fill.
         * Entire card is the click target. Designed for Phase 2 drag-and-drop
         * attachment (D-01) — same element, same position.
         */}
        <div
          role="button"
          tabIndex={0}
          aria-label="Open Parquet file"
          onClick={() => void handleOpen()}
          onKeyDown={handleKeyDown}
          style={{
            width: "480px",
            height: "200px",
            border: "2px dashed var(--primary)",
            backgroundColor: "var(--card)",
            borderRadius: "var(--radius)",
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            gap: "8px",
            cursor: "pointer",
            userSelect: "none",
            transition: "transform 0.1s ease, border-color 0.1s ease",
            outline: "none",
          }}
          onMouseEnter={(e) => {
            (e.currentTarget as HTMLElement).style.transform = "scale(1.01)";
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLElement).style.transform = "scale(1)";
          }}
          onMouseDown={(e) => {
            (e.currentTarget as HTMLElement).style.transform = "scale(0.99)";
          }}
          onMouseUp={(e) => {
            (e.currentTarget as HTMLElement).style.transform = "scale(1.01)";
          }}
          onFocus={(e) => {
            (e.currentTarget as HTMLElement).style.outline =
              "2px solid var(--ring)";
            (e.currentTarget as HTMLElement).style.outlineOffset = "2px";
          }}
          onBlur={(e) => {
            (e.currentTarget as HTMLElement).style.outline = "none";
          }}
        >
          {/* Heading: 16px / 600 weight (UI-SPEC.md §Typography) */}
          <span
            style={{
              fontSize: "16px",
              fontWeight: 600,
              color: "var(--foreground)",
              lineHeight: 1.2,
            }}
          >
            Open Parquet File
          </span>
          {/* Subtitle: body text (UI-SPEC.md §Copywriting — no Phase-2 parenthetical) */}
          <span
            style={{
              fontSize: "14px",
              fontWeight: 400,
              color: "var(--muted-foreground)",
              lineHeight: 1.5,
            }}
          >
            Click to browse for a .parquet file
          </span>
        </div>

        {/*
         * Secondary affordance: ghost link-style button below the card (REMOTE-01 / D-02).
         * 14px, --muted-foreground color signals a secondary action.
         * UI-SPEC.md §Component Inventory: "Open remote file" (lowercase).
         */}
        <button
          onClick={onOpenRemote}
          style={{
            background: "none",
            border: "none",
            color: "var(--muted-foreground)",
            fontSize: "14px",
            cursor: "pointer",
            textDecoration: "underline",
            textUnderlineOffset: "3px",
            padding: "4px 8px",
          }}
        >
          Open remote file
        </button>
      </div>
    </div>
  );
}
