import * as React from "react";
import { cn } from "@/lib/utils";

/**
 * InputProps extends the full HTML input element attribute set.
 * Pass `type="password"` for secret fields (e.g. secret_access_key reveal toggle).
 * No CVA variants needed — single variant; the `type` prop handles password/text toggle.
 */
export interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {}

/**
 * Input UI primitive following the button.tsx CVA/cn/forwardRef pattern.
 *
 * Styled consistently with the app's design system: uses CSS custom properties
 * (`--border`, `--muted-foreground`, `--ring`) so it adapts to theme changes.
 * No external style registry pull — hand-written to match button.tsx conventions.
 */
const Input = React.forwardRef<HTMLInputElement, InputProps>(
  ({ className, type, ...props }, ref) => {
    return (
      <input
        type={type}
        className={cn(
          "flex h-9 w-full rounded-md border border-[--border] bg-transparent px-3 py-1 text-sm shadow-sm transition-colors file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-[--muted-foreground] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[--ring] disabled:cursor-not-allowed disabled:opacity-50",
          className
        )}
        ref={ref}
        {...props}
      />
    );
  }
);
Input.displayName = "Input";

export { Input };
