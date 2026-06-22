/**
 * Row Groups tab — compact per-row-group breakdown table (META-03 / D-07).
 *
 * Reads `fileMetadata.row_groups` from the store and renders a flat table with
 * columns [# | rows | size | codec], one row per group. A "Row groups: N" total
 * is shown above the table (D-07).
 *
 * Scope fence (CONTEXT.md §Explicitly NOT in Phase 3):
 *   - No per-column statistics (DISP-03) — only per-row-group rows/size/codec.
 *   - No encoding/page detail (DISP-04) — only the top-level compression codec.
 *
 * Security: codec strings are untrusted backend text rendered as React text children.
 * No dangerouslySetInnerHTML anywhere in this component (T-03-04).
 *
 * Source: 03-02-PLAN.md §Task 1 / 03-PATTERNS.md §RowGroupsTab / 03-CONTEXT.md §D-07, D-08
 */
import { useShallow } from "zustand/react/shallow";
import { useAppStore } from "@/store/appStore";
import { formatCount, formatBytes } from "@/lib/format";

export function RowGroupsTab() {
  const { fileMetadata } = useAppStore(
    useShallow((s) => ({ fileMetadata: s.fileMetadata }))
  );

  // Empty state: no metadata loaded yet, or row_groups array is empty.
  if (!fileMetadata || fileMetadata.row_groups.length === 0) {
    return (
      <div
        style={{
          padding: "16px 10px",
          fontSize: "12px",
          color: "var(--muted-foreground)",
          textAlign: "center",
        }}
      >
        No row-group metadata available.
      </div>
    );
  }

  const groups = fileMetadata.row_groups;

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        overflow: "hidden",
      }}
    >
      {/* "Row groups: N" total label (D-07) */}
      <div
        style={{
          padding: "6px 10px",
          flexShrink: 0,
          fontSize: "11px",
          color: "var(--muted-foreground)",
          borderBottom: "1px solid var(--border)",
          backgroundColor: "var(--muted)",
        }}
      >
        Row groups: {groups.length}
      </div>

      {/* Compact table — one row per group (D-07) */}
      <div style={{ flex: 1, overflow: "auto" }}>
        <table
          style={{
            borderCollapse: "collapse",
            width: "100%",
            fontSize: "12px",
          }}
        >
          <thead>
            <tr>
              {["#", "rows", "size", "codec"].map((h) => (
                <th
                  key={h}
                  style={{
                    padding: "4px 8px",
                    textAlign: "left",
                    borderBottom: "1px solid var(--border)",
                    color: "var(--muted-foreground)",
                    fontWeight: 400,
                    backgroundColor: "var(--background)",
                    position: "sticky",
                    top: 0,
                  }}
                >
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {groups.map((rg, i) => (
              <tr key={i}>
                {/* Group index (1-based) */}
                <td
                  style={{
                    padding: "4px 8px",
                    borderBottom: "1px solid var(--border)",
                    color: "var(--muted-foreground)",
                    flexShrink: 0,
                  }}
                >
                  {i + 1}
                </td>
                {/* Row count — thousands-separated (D-08) */}
                <td
                  style={{
                    padding: "4px 8px",
                    borderBottom: "1px solid var(--border)",
                    color: "var(--foreground)",
                  }}
                >
                  {formatCount(rg.num_rows)}
                </td>
                {/* Byte size — auto-scaled KB/MB/GB (D-08) */}
                <td
                  style={{
                    padding: "4px 8px",
                    borderBottom: "1px solid var(--border)",
                    color: "var(--foreground)",
                  }}
                >
                  {formatBytes(rg.total_byte_size)}
                </td>
                {/* Compression codec — untrusted text rendered as React child (T-03-04) */}
                <td
                  style={{
                    padding: "4px 8px",
                    borderBottom: "1px solid var(--border)",
                    color: "var(--foreground)",
                  }}
                >
                  {rg.compression}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
