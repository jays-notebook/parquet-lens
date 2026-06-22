/**
 * Augment React's HTMLAttributes to include the `inert` boolean attribute.
 *
 * `inert` is an HTML global attribute that prevents all user interaction with
 * an element and its subtree. It was added to the HTML spec in 2023 and is
 * supported by all modern browsers but not yet typed in @types/react 18.
 *
 * This augmentation uses module augmentation (not module replacement) so it
 * extends the existing React types rather than replacing them.
 *
 * Value convention:
 *   - inert=""        → attribute present (element is inert)
 *   - inert={undefined} → attribute absent (React omits it)
 *
 * Ref: https://developer.mozilla.org/en-US/docs/Web/HTML/Global_attributes/inert
 */
import "react";

declare module "react" {
  interface HTMLAttributes<T> {
    inert?: "" | undefined;
  }
}
