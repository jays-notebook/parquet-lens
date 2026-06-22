/**
 * Row count chip — displays the RESULT row count or 100-row cap notice.
 *
 * Phase 1: shown when results exist (totalRows > 0).
 * Height: 32px bar between RunBar and ResultsGrid (UI-SPEC.md §Layout Contract).
 * Copy logic (driven by `capped` from RunQueryResponse — D-04):
 *   capped === true  → "Showing first 100+ rows (more available)"
 *   capped === false → "Showing {N} rows"
 *
 * When capped, the exact total is intentionally NOT shown: the backend stops the result
 * stream at the 100-row cap and never materializes the full set (CLAUDE.md performance
 * constraint), so the true total is unknown without a separate COUNT query. The "100+"
 * form signals "more rows exist" without that extra query.
 *
 * Role separation (D-10/D-11): this chip reports the QUERY RESULT count ONLY.
 * The FILE total row count (from the Parquet footer) lives in SchemaPanel's fixed header.
 * These two numbers must NEVER be conflated — a WHERE/aggregation result is not a subset
 * of the whole file from the user's perspective.
 *
 * Source: UI-SPEC.md §100-Row Cap Notice, §Copywriting Contract, §Accessibility Contract
 */
import { useShallow } from "zustand/react/shallow";
import { Badge } from "@/components/ui/badge";
import { useAppStore } from "@/store/appStore";

export function RowCapChip() {
  const { totalRows, capped } = useAppStore(useShallow((s) => ({
    totalRows: s.totalRows,
    capped: s.capped,
  })));

  const label = capped
    ? "Showing first 100+ rows (more available)"
    : `Showing ${totalRows} rows`;

  return (
    <div
      style={{
        height: "32px",
        display: "flex",
        alignItems: "center",
        paddingLeft: "16px",
        paddingRight: "16px",
        flexShrink: 0,
        backgroundColor: "var(--background)",
        borderBottom: "1px solid var(--border)",
      }}
    >
      <Badge
        variant="secondary"
        aria-label="Row count notice"
        style={{
          fontSize: "12px",
          fontWeight: 400,
          color: "var(--muted-foreground)",
          backgroundColor: "var(--muted)",
          border: "1px solid var(--border)",
          padding: "2px 8px",
        }}
      >
        {label}
      </Badge>
    </div>
  );
}
