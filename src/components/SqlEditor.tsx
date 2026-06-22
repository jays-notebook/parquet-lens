/**
 * CodeMirror 6 SQL editor wrapper.
 *
 * Height: 160px fixed (UI-SPEC.md Screen B layout).
 * Unfocused: --border outline 1px.
 * Focused: --ring outline 2px, 2px offset (accent blue-500).
 * Ctrl+Enter (Mod-Enter) keybinding fires the `onRun` prop callback (QUERY-02).
 * Font: 13px / weight 400 (UI-SPEC.md §Typography Code role).
 *
 * Source: STACK.md §CodeMirror 6, UI-SPEC.md §SQL Editor Focus, §Accessibility Contract
 */
import { useEffect, useRef } from "react";
import { Compartment } from "@codemirror/state";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { defaultKeymap } from "@codemirror/commands";
import { sql } from "@codemirror/lang-sql";

interface SqlEditorProps {
  /** Current SQL text (controlled). */
  value: string;
  /** Called when the SQL text changes. */
  onChange: (sql: string) => void;
  /** Called when the user triggers a run (Ctrl+Enter or Run button). */
  onRun: () => void;
  /** Whether the editor should be read-only (e.g. while a query is running). */
  disabled?: boolean;
}

export function SqlEditor({ value, onChange, onRun, disabled }: SqlEditorProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  // Compartment allows dynamic reconfiguration of the editable extension.
  const editableCompartment = useRef(new Compartment());
  const onRunRef = useRef(onRun);
  const onChangeRef = useRef(onChange);

  // Keep refs current without re-creating the editor.
  onRunRef.current = onRun;
  onChangeRef.current = onChange;

  useEffect(() => {
    if (!containerRef.current) return;

    const view = new EditorView({
      state: EditorState.create({
        doc: value,
        extensions: [
          sql(),
          // Editable state managed via Compartment for dynamic updates.
          editableCompartment.current.of(EditorView.editable.of(!disabled)),
          // Ctrl+Enter / Cmd+Enter triggers the run callback (QUERY-02).
          keymap.of([
            {
              key: "Mod-Enter",
              run: () => {
                onRunRef.current();
                return true;
              },
            },
            ...defaultKeymap,
          ]),
          // Dispatch SQL changes back to the controlled parent.
          EditorView.updateListener.of((update) => {
            if (update.docChanged) {
              onChangeRef.current(update.state.doc.toString());
            }
          }),
          // Base theme: font size + dark background matching the design system.
          EditorView.theme({
            "&": {
              fontSize: "13px",
              fontFamily:
                "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
              fontWeight: "400",
              lineHeight: "1.6",
              height: "160px",
              backgroundColor: "var(--muted)",
              color: "var(--foreground)",
            },
            ".cm-scroller": {
              overflow: "auto",
              height: "160px",
            },
            ".cm-content": {
              caretColor: "var(--foreground)",
              padding: "8px 12px",
            },
            ".cm-focused": {
              outline: "none",
            },
            ".cm-line": {
              lineHeight: "1.6",
            },
            // Style the cursor to be visible on dark background.
            ".cm-cursor": {
              borderLeftColor: "var(--foreground)",
            },
          }),
        ],
      }),
      parent: containerRef.current,
    });

    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // Only create the editor once on mount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Sync value from parent when it changes externally (e.g., setFile resets queryText).
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const current = view.state.doc.toString();
    if (current !== value) {
      view.dispatch({
        changes: { from: 0, to: current.length, insert: value },
      });
    }
  }, [value]);

  // Sync disabled/editable state via Compartment reconfiguration.
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: editableCompartment.current.reconfigure(
        EditorView.editable.of(!disabled)
      ),
    });
  }, [disabled]);

  return (
    <div
      ref={containerRef}
      aria-label="SQL query editor"
      style={{
        // Outline: unfocused = --border 1px, focused = --ring 2px with 2px offset.
        outline: "1px solid var(--border)",
        outlineOffset: "0px",
        overflow: "hidden",
        height: "160px",
        flexShrink: 0,
        opacity: disabled ? 0.6 : 1,
        transition: "opacity 0.15s ease",
      }}
      onFocus={() => {
        if (containerRef.current) {
          containerRef.current.style.outline = "2px solid var(--ring)";
          containerRef.current.style.outlineOffset = "2px";
        }
      }}
      onBlur={() => {
        if (containerRef.current) {
          containerRef.current.style.outline = "1px solid var(--border)";
          containerRef.current.style.outlineOffset = "0px";
        }
      }}
    />
  );
}
