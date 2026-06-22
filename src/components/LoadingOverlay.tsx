/**
 * Loading overlay for the results grid area.
 *
 * Positioned absolutely over the results grid; centered Loader2 24px animate-spin.
 * Grid content beneath becomes opacity: 0.3.
 * Visible only when `isLoading` is true.
 * No text label — spinner only (UI-SPEC.md §Copywriting: "no text — spinner only").
 *
 * aria-live and aria-busy are set on the grid container (ResultsGrid), not here.
 *
 * Source: UI-SPEC.md §Loading State, §Copywriting Contract
 */
import { Loader2 } from "lucide-react";

interface LoadingOverlayProps {
  visible: boolean;
}

export function LoadingOverlay({ visible }: LoadingOverlayProps) {
  if (!visible) return null;

  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        // Semi-transparent backdrop — content underneath shows at opacity 0.3
        // (the grid itself sets opacity:0.3 when isLoading; this overlay sits on top).
        pointerEvents: "none",
        zIndex: 10,
      }}
    >
      <Loader2
        size={24}
        className="animate-spin"
        style={{ color: "var(--muted-foreground)" }}
      />
    </div>
  );
}
