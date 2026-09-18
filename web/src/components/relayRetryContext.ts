import { createContext, useContext } from "react";

/**
 * Bumped every time "Try again" is clicked on the nearest RelayErrorBoundary;
 * see `useRetryableLazyLoadQuery` for why a component needs this to actually
 * retry rather than rethrow a cached error.
 */
export const RelayRetryContext = createContext(0);

/**
 * Returns 0 (a no-op fetch key) outside any RelayErrorBoundary, so it's safe
 * to call unconditionally.
 */
export function useRelayRetryFetchKey(): number {
  return useContext(RelayRetryContext);
}
