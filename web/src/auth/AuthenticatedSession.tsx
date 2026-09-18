import { Suspense, startTransition, useState, type ReactNode } from "react";
import { rawGraphQL } from "../lib/rawGraphQL";
import {
  getSessionToken,
  setSessionToken,
  clearSessionToken,
} from "../lib/sessionToken";
import AuthenticatedRelayEnvironment from "./AuthenticatedRelayEnvironment";
import { CurrentUserProvider } from "./CurrentUserProvider";
import LoginPage from "./LoginPage";
import { LogoutContext } from "./useLogout";
import LoadingIndicator from "../components/LoadingIndicator";
import RelayErrorBoundary from "../components/RelayErrorBoundary";

// Session state as a single machine so invalid flag combinations (e.g.
// "authenticated" and "showing the login page") can't occur.
type Status =
  | { kind: "authenticated" }
  | { kind: "loggingOut" }
  | { kind: "unauthenticated"; error: string | null };

/**
 * The wrapper-component pattern, not a route guard: this decides *what to
 * render* rather than redirecting. Unauthenticated → `LoginPage` in place of
 * `children`. Authenticated → the provider stack every `/app/*` page needs:
 * Relay environment → error boundary → suspense → current-user context.
 */
export default function AuthenticatedSession({
  children,
}: {
  children: ReactNode;
}) {
  const [status, setStatus] = useState<Status>(() =>
    getSessionToken()
      ? { kind: "authenticated" }
      : { kind: "unauthenticated", error: null },
  );

  // The server rejected our token (expired or revoked) — a definitive 401.
  // Transient 5xx/network failures never reach here (see `fetchGraphQL`), so
  // this never drops a still-valid token over a blip.
  function onUnauthorized() {
    clearSessionToken();
    setStatus({
      kind: "unauthenticated",
      error: "Your session has expired. Please log in again.",
    });
  }

  // Relay couldn't obtain a token to send (getToken threw). Shouldn't
  // normally happen: the authenticated tree only mounts once a token
  // exists, so reaching here means it vanished mid-session.
  function onTokenError() {
    setStatus({
      kind: "unauthenticated",
      error: "Something went wrong with your session. Please log in again.",
    });
  }

  function onNewTokenReceived(token: string) {
    setSessionToken(token);
    startTransition(() => {
      setStatus({ kind: "authenticated" });
    });
  }

  async function onLogout() {
    // Switch to the loading view immediately so we don't briefly re-mount
    // LoginPage (which would kick off a wasted passkey autofill ceremony)
    // while the logout request is in flight.
    setStatus({ kind: "loggingOut" });
    const token = getSessionToken();
    if (token) {
      try {
        await rawGraphQL("mutation Logout { logout }", {}, `Bearer ${token}`);
      } catch {
        // Ignore — the token will expire via TTL regardless.
      }
    }
    clearSessionToken();
    setStatus({ kind: "unauthenticated", error: null });
  }

  if (status.kind === "loggingOut") {
    return <LoadingIndicator />;
  }

  if (status.kind === "unauthenticated") {
    return (
      <LoginPage
        errorMessage={status.error}
        onNewTokenReceived={onNewTokenReceived}
      />
    );
  }

  return (
    <AuthenticatedRelayEnvironment
      onTokenError={onTokenError}
      onUnauthorized={onUnauthorized}
    >
      <RelayErrorBoundary canRetry>
        <Suspense fallback={<LoadingIndicator />}>
          <CurrentUserProvider>
            <LogoutContext value={onLogout}>{children}</LogoutContext>
          </CurrentUserProvider>
        </Suspense>
      </RelayErrorBoundary>
    </AuthenticatedRelayEnvironment>
  );
}
