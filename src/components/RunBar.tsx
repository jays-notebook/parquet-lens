/**
 * Run bar — 40px bar with the Run Query button and Ctrl+Enter shortcut hint.
 *
 * While isLoading:
 *   - Button is disabled (--muted fill, --muted-foreground text, cursor: not-allowed).
 *   - Loader2 icon (16px, animate-spin) replaces the play icon.
 *   - aria-disabled="true" is set to prevent duplicate execution (D-03).
 *
 * Source: UI-SPEC.md §Run Button interaction states, §Copywriting Contract, §Accessibility Contract
 */
import { useShallow } from "zustand/react/shallow";
import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useAppStore } from "@/store/appStore";
import { useRunQuery } from "@/hooks/useRunQuery";

export function RunBar() {
  const { isLoading, registrationStatus } = useAppStore(useShallow((s) => ({
    isLoading: s.isLoading,
    // Phase 4 (D-03/LOAD-02): gate Run button and handler on registration state.
    registrationStatus: s.registrationStatus,
  })));

  // Shared run-query handler (WR-01) — same implementation SqlEditor's onRun uses.
  const { runQueryHandler: handleRun } = useRunQuery();

  return (
    <div
      style={{
        height: "40px",
        display: "flex",
        alignItems: "center",
        gap: "12px",
        paddingLeft: "16px",
        paddingRight: "16px",
        flexShrink: 0,
        backgroundColor: "var(--background)",
        borderBottom: "1px solid var(--border)",
      }}
    >
      <Button
        onClick={handleRun}
        // Phase 4 (D-03/LOAD-02): disabled until file is registered AND no query in flight.
        disabled={isLoading || registrationStatus !== "registered"}
        aria-label="Run query"
        // aria-disabled mirrors disabled so assistive tech announces the state.
        aria-disabled={isLoading || registrationStatus !== "registered"}
        style={
          isLoading || registrationStatus !== "registered"
            ? {
                backgroundColor: "var(--muted)",
                color: "var(--muted-foreground)",
                cursor: "not-allowed",
                border: "none",
                display: "flex",
                alignItems: "center",
                gap: "6px",
              }
            : {
                backgroundColor: "var(--primary)",
                color: "#ffffff",
                border: "none",
                display: "flex",
                alignItems: "center",
                gap: "6px",
              }
        }
      >
        {isLoading && <Loader2 size={16} className="animate-spin" />}
        Run Query
      </Button>

      {/* Ctrl+Enter shortcut hint — grays out when Run is inactive (UI-SPEC.md §RunBar) */}
      <span
        style={{
          fontSize: "12px",
          // Phase 4: gray out hint when not registered OR loading — shortcut is non-functional.
          color: (isLoading || registrationStatus !== "registered")
            ? "var(--muted-foreground)"
            : "var(--primary)",
          userSelect: "none",
        }}
      >
        Ctrl+Enter
      </span>
    </div>
  );
}
