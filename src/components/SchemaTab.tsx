/**
 * Schema tab — compact column list with name, Arrow-native type, and nullability.
 *
 * Reads `schema: SchemaField[]` from the store (populated on file open by setFile).
 * Provides a case-insensitive substring filter box (D-13).
 *
 * Each column row:
 *   - Truncated name with native title tooltip (mirrors ResultsGrid pattern, D-12)
 *   - Arrow-native type name in muted 11px (locked from Phase 1/2 — no SQL aliases, DISP-01)
 *   - Nullability badge: "null" (secondary) or "NN" (outline)
 *
 * All metadata rendered as React text children — no dangerouslySetInnerHTML (T-03-04).
 * Uses CSS variables only — no hard-coded colors (CONVENTIONS.md).
 *
 * Source: 03-01-PLAN.md §Task 2 / 03-PATTERNS.md §SchemaTab
 */
import { useState } from "react";
import { useAppStore } from "@/store/appStore";
import { Badge } from "@/components/ui/badge";

export function SchemaTab() {
  const schema = useAppStore((s) => s.schema);
  const [filter, setFilter] = useState("");

  const visible = schema.filter((f) =>
    f.name.toLowerCase().includes(filter.toLowerCase())
  );

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        overflow: "hidden",
      }}
    >
      {/* Filter box (D-13) */}
      <div
        style={{
          padding: "6px 8px",
          borderBottom: "1px solid var(--border)",
          flexShrink: 0,
        }}
      >
        <input
          type="text"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Filter columns…"
          style={{
            width: "100%",
            boxSizing: "border-box",
            fontSize: "12px",
            padding: "4px 6px",
            border: "1px solid var(--border)",
            borderRadius: "4px",
            backgroundColor: "var(--background)",
            color: "var(--foreground)",
            outline: "none",
          }}
        />
      </div>

      {/* Column list — vertically scrollable (D-12 / D-13) */}
      <div
        style={{
          flex: 1,
          overflowY: "auto",
        }}
      >
        {visible.length === 0 ? (
          <div
            style={{
              padding: "12px 8px",
              fontSize: "12px",
              color: "var(--muted-foreground)",
              textAlign: "center",
            }}
          >
            {schema.length === 0 ? "No schema loaded" : "No columns match"}
          </div>
        ) : (
          visible.map((field) => (
            <div
              key={field.name}
              style={{
                display: "flex",
                alignItems: "center",
                gap: "6px",
                padding: "4px 8px",
                borderBottom: "1px solid var(--border)",
                minWidth: 0,
              }}
            >
              {/* Column name — truncated with full-detail tooltip (D-12) */}
              <span
                title={`${field.name} (${field.arrow_type}${field.nullable ? ", nullable" : ", not null"})`}
                style={{
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                  flex: 1,
                  fontSize: "13px",
                  color: "var(--foreground)",
                  minWidth: 0,
                }}
              >
                {field.name}
              </span>

              {/* Arrow-native type — dimmed, locked from Phase 1/2 (DISP-01 boundary) */}
              <span
                style={{
                  fontSize: "11px",
                  color: "var(--muted-foreground)",
                  flexShrink: 0,
                  fontFamily: "monospace",
                }}
              >
                {field.arrow_type}
              </span>

              {/* Nullability badge */}
              <Badge
                variant={field.nullable ? "secondary" : "outline"}
                style={{
                  fontSize: "10px",
                  padding: "1px 4px",
                  flexShrink: 0,
                  lineHeight: 1.4,
                }}
              >
                {field.nullable ? "null" : "NN"}
              </Badge>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
