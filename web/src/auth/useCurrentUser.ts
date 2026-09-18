import { useContext } from "react";
import { CurrentUserContext } from "./CurrentUserContext";

/**
 * The authenticated user's own record — plain fields plus fragment refs any
 * descendant component can spread its own `useFragment` call against (see
 * `CurrentUserProvider`). Must be called within `CurrentUserProvider`.
 */
export function useCurrentUser() {
  const context = useContext(CurrentUserContext);
  if (context === undefined) {
    throw new Error("useCurrentUser must be used within a CurrentUserProvider");
  }
  return context;
}
