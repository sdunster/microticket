import { useState, type ReactNode } from "react";
import { ErrorBoundary, type FallbackProps } from "react-error-boundary";
import { commitLocalUpdate } from "relay-runtime";
import { useRelayEnvironment } from "react-relay";
import PageErrorFallback from "./PageErrorFallback";
import { RelayRetryContext } from "./relayRetryContext";

interface RelayErrorBoundaryProps {
  children: ReactNode;
  /** Remounts the boundary (clearing its caught error) when this value changes. */
  resetKey?: string | number;
  showDetailsByDefault?: boolean;
  /**
   * Set only once every `useLazyLoadQuery` call reachable from `children`
   * threads `useRelayRetryFetchKey()` into its `fetchKey` (via
   * `useRetryableLazyLoadQuery`) — that's what makes "Try again" actually
   * retry (see the doc comment below). Without it, "Try again" looks like
   * it does something but doesn't: default to a "Reload page" button that
   * does a real `window.location.reload()` instead.
   */
  canRetry?: boolean;
}

/**
 * An ErrorBoundary for use anywhere inside a `RelayEnvironmentProvider`.
 *
 * A bare `resetErrorBoundary` only resets React state, and invalidating the
 * store on reset isn't enough either: `useLazyLoadQuery`'s underlying
 * QueryResource caches the outcome of a query — success, pending promise, or
 * thrown error — keyed by (fetchPolicy, renderPolicy, operation identifier),
 * independent of the store's invalidation epoch. On retry with the same
 * variables, that cache entry still holds the original error and is
 * rethrown synchronously with no network request. So on reset:
 *  - `store.invalidateStore()` bumps the store's invalidation epoch (so a
 *    fresh cache entry treats existing store data as stale).
 *  - `retryGeneration` increments and flows through `RelayRetryContext`, so
 *    a query component that threads it into `fetchKey`
 *    (`useRetryableLazyLoadQuery`) gets a fresh QueryResource cache entry —
 *    which is what actually triggers a new fetch.
 *
 * `canRetry` defaults to `false` so a boundary that hasn't been checked for
 * that shows an honest "Reload page" instead of a "Try again" that silently
 * rethrows the same cached error.
 */
export default function RelayErrorBoundary({
  children,
  resetKey,
  showDetailsByDefault,
  canRetry = false,
}: RelayErrorBoundaryProps) {
  const environment = useRelayEnvironment();
  const [retryGeneration, setRetryGeneration] = useState(0);

  return (
    <ErrorBoundary
      key={resetKey}
      onReset={() => {
        commitLocalUpdate(environment, (store) => store.invalidateStore());
        setRetryGeneration((n) => n + 1);
      }}
      fallbackRender={({ error, resetErrorBoundary }: FallbackProps) => (
        <PageErrorFallback
          error={error}
          resetErrorBoundary={resetErrorBoundary}
          showDetailsByDefault={showDetailsByDefault}
          reloadInstead={!canRetry}
        />
      )}
    >
      <RelayRetryContext.Provider value={retryGeneration}>
        {children}
      </RelayRetryContext.Provider>
    </ErrorBoundary>
  );
}
