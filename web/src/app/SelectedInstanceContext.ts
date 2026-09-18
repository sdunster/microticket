import { createContext, useContext } from "react";
import type { SelectedInstance } from "./selectedInstance";

/**
 * The instance switcher's current selection, threaded down to nested `/app`
 * routes (the ticket-list pages 8b adds) so they know which instance to
 * scope their queries to without each one re-deriving it.
 */
export const SelectedInstanceContext = createContext<SelectedInstance | null>(
  null,
);

/** `null` while memberships haven't loaded yet or the user has none. */
export function useSelectedInstance(): SelectedInstance | null {
  return useContext(SelectedInstanceContext);
}
