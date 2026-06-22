/**
 * Schema/Metadata left sidebar shell — Screen B only (D-01 / D-04).
 *
 * Layout (D-01..D-07):
 *   - Fixed width ~280px when open, ~32px when collapsed (D-03)
 *   - Fixed header: file basename + total row count from Parquet footer (D-06 / META-02)
 *   - Tab bar: [Schema | Row Groups] with Schema default (D-05)
 *   - Tab content area: flex-grow, overflow:auto
 *   - Collapse toggle: shows only toggle button when collapsed (D-02)
 *
 * Security: all untrusted strings (column names, codec, file name) rendered as React
 * text children — no dangerouslySetInnerHTML (T-03-04).
 *
 * Source: 03-01-PLAN.md §Task 2 / 03-PATTERNS.md §SchemaPanel / 03-CONTEXT.md §D-01..D-07
 */
import { useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { useAppStore } from "@/store/appStore";
import { SchemaTab } from "./SchemaTab";
import { RowGroupsTab } from "./RowGroupsTab";

type TabId = "schema" | "row-groups";

/** Extracts the filename from an absolute path (Windows or Unix). */
function basename(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

/** Formats a number with thousands separators (D-08). */
function formatCount(n: number): string {
  return Number(n).toLocaleString();
}

export function SchemaPanel() {
  const { fileMetadata, sidebarCollapsed, setSidebarCollapsed, filePath, schema } =
    useAppStore(
      useShallow((s) => ({
        fileMetadata: s.fileMetadata,
        sidebarCollapsed: s.sidebarCollapsed,
        setSidebarCollapsed: s.setSidebarCollapsed,
        filePath: s.filePath,
        schema: s.schema,
      }))
    );

  const [activeTab, setActiveTab] = useState<TabId>("schema");

  const filename = filePath ? basename(filePath) : "";
  const totalRows = fileMetadata?.total_rows ?? null;

  return (
    <div
      style={{
        width: sidebarCollapsed ? "32px" : "280px",
        flexShrink: 0,
        height: "100%",
        overflow: "hidden",
        borderRight: "1px solid var(--border)",
        backgroundColor: "var(--background)",
        display: "flex",
        flexDirection: "column",
        transition: "width 0.15s ease",
      }}
    >
      {/* Collapse toggle — always visible */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: sidebarCollapsed ? "center" : "flex-end",
          padding: "4px",
          flexShrink: 0,
          borderBottom: sidebarCollapsed ? "none" : "1px solid var(--border)",
        }}
      >
        <button
          onClick={() => setSidebarCollapsed(!sidebarCollapsed)}
          title={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
          style={{
            width: "24px",
            height: "24px",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            border: "none",
            borderRadius: "4px",
            backgroundColor: "transparent",
            cursor: "pointer",
            color: "var(--muted-foreground)",
            fontSize: "14px",
            padding: 0,
          }}
          aria-label={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
        >
          {sidebarCollapsed ? "›" : "‹"}
        </button>
      </div>

      {/* Expanded content — hidden when collapsed */}
      {!sidebarCollapsed && (
        <>
          {/* Fixed header: file name + total row count (D-06 / META-02) */}
          <div
            style={{
              padding: "8px 10px",
              flexShrink: 0,
              borderBottom: "1px solid var(--border)",
              backgroundColor: "var(--muted)",
            }}
          >
            {/* File basename — truncated with full path in tooltip (D-06) */}
            <div
              title={filePath ?? ""}
              style={{
                fontSize: "12px",
                fontWeight: 500,
                color: "var(--foreground)",
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
                marginBottom: "2px",
              }}
            >
              {filename}
            </div>

            {/* File stats row: total rows + column count */}
            <div
              style={{
                display: "flex",
                gap: "8px",
                fontSize: "11px",
                color: "var(--muted-foreground)",
              }}
            >
              {totalRows !== null ? (
                <span title="Total rows in file (from Parquet footer)">
                  {formatCount(totalRows)} rows
                </span>
              ) : (
                <span>Loading…</span>
              )}
              {schema.length > 0 && (
                <span title="Number of columns">
                  {schema.length} cols
                </span>
              )}
            </div>
          </div>

          {/* Tab bar (D-05) */}
          <div
            style={{
              display: "flex",
              flexShrink: 0,
              borderBottom: "1px solid var(--border)",
              backgroundColor: "var(--background)",
            }}
          >
            {(["schema", "row-groups"] as TabId[]).map((tab) => (
              <button
                key={tab}
                onClick={() => setActiveTab(tab)}
                style={{
                  flex: 1,
                  padding: "6px 4px",
                  fontSize: "12px",
                  fontWeight: activeTab === tab ? 500 : 400,
                  color: activeTab === tab ? "var(--foreground)" : "var(--muted-foreground)",
                  backgroundColor: "transparent",
                  border: "none",
                  borderBottom: activeTab === tab ? "2px solid var(--foreground)" : "2px solid transparent",
                  cursor: "pointer",
                  textTransform: "capitalize",
                }}
                aria-selected={activeTab === tab}
              >
                {tab === "schema" ? "Schema" : "Row Groups"}
              </button>
            ))}
          </div>

          {/* Tab content area — flex-grow, overflow:auto */}
          <div
            style={{
              flex: 1,
              overflow: "hidden",
              display: "flex",
              flexDirection: "column",
            }}
          >
            {activeTab === "schema" ? (
              <SchemaTab />
            ) : (
              <RowGroupsTab />
            )}
          </div>
        </>
      )}
    </div>
  );
}
