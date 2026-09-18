import { RelayEnvironmentProvider } from "react-relay";
import { Suspense, useCallback, useMemo, type ReactNode } from "react";
import { createAuthenticatedGraphQLEnvironment } from "../lib/environments";
import LoadingIndicator from "../components/LoadingIndicator";
import { getSessionToken } from "../lib/sessionToken";

/**
 * Relay environment provider for `/app/*`. Uses the stored session token;
 * if there's none, `getToken` throws and the caller's `onTokenError` sends
 * the user back to the login screen.
 */
export default function AuthenticatedRelayEnvironment({
  onUnauthorized,
  onTokenError,
  children,
}: {
  onUnauthorized: () => void;
  onTokenError: () => void;
  children: ReactNode;
}) {
  const getToken = useCallback(async () => {
    const stored = getSessionToken();
    if (!stored) throw new Error("No session token");
    return stored;
  }, []);

  // Pass getToken directly so it's called lazily per-request. This keeps
  // the Environment object stable across renders, preserving the Relay
  // store and preventing a Suspense remount loop.
  const environment = useMemo(
    () =>
      createAuthenticatedGraphQLEnvironment(
        getToken,
        onTokenError,
        onUnauthorized,
      ),
    [getToken, onTokenError, onUnauthorized],
  );

  // The Suspense boundary must be INSIDE RelayEnvironmentProvider so that
  // when a descendant suspends, this component (and its Environment) stays
  // mounted. If Suspense were above this component, every suspension would
  // unmount the Environment and wipe the Relay store, restarting the loop.
  return (
    <RelayEnvironmentProvider environment={environment}>
      <Suspense fallback={<LoadingIndicator />}>{children}</Suspense>
    </RelayEnvironmentProvider>
  );
}
