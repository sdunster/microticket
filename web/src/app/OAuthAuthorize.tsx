import { Suspense, useState } from "react";
import { useSearchParams } from "react-router";
import { graphql, useMutation } from "react-relay";
import { useRetryableLazyLoadQuery } from "../components/useRetryableLazyLoadQuery";
import RelayErrorBoundary from "../components/RelayErrorBoundary";
import LoadingIndicator from "../components/LoadingIndicator";
import { useCurrentUser } from "../auth/useCurrentUser";
import { relayMutationErrorMessage } from "../lib/relayMutationError";
import { Card } from "../components/ui/Card";
import { Button } from "../components/ui/Button";
import type { OAuthAuthorizeQuery } from "./__generated__/OAuthAuthorizeQuery.graphql";
import type { OAuthAuthorizeMutation } from "./__generated__/OAuthAuthorizeMutation.graphql";

/** Everything the consent screen needs out of the query string, once shape-checked. */
interface AuthorizationParams {
  clientId: string;
  redirectUri: string;
  codeChallenge: string;
  codeChallengeMethod: string;
  scope: string | null;
  resource: string | null;
  state: string | null;
}

/**
 * Read and shape-check the OAuth params from the query string. Doesn't touch
 * the network — an unknown client or unregistered redirect_uri is caught
 * later, by the `oauthAuthorizationRequest` query itself.
 */
function parseParams(search: URLSearchParams): AuthorizationParams | null {
  const clientId = search.get("client_id");
  const redirectUri = search.get("redirect_uri");
  const codeChallenge = search.get("code_challenge");
  const codeChallengeMethod = search.get("code_challenge_method");
  if (
    !clientId ||
    !redirectUri ||
    !codeChallenge ||
    !codeChallengeMethod ||
    search.get("response_type") !== "code"
  ) {
    return null;
  }
  return {
    clientId,
    redirectUri,
    codeChallenge,
    codeChallengeMethod,
    scope: search.get("scope"),
    resource: search.get("resource"),
    state: search.get("state"),
  };
}

/** Append `error=access_denied` (+ `state`, if given) to an already-validated redirect_uri. */
function denialUrl(redirectUri: string, state: string | null): string {
  const url = new URL(redirectUri);
  url.searchParams.set("error", "access_denied");
  if (state) url.searchParams.set("state", state);
  return url.toString();
}

function Notice({ title, children }: { title: string; children: string }) {
  return (
    <Card className="mx-auto mt-16 max-w-lg">
      <h1 className="text-xl font-semibold text-ink-strong">{title}</h1>
      <p className="mt-3 text-ink-muted">{children}</p>
    </Card>
  );
}

/**
 * `/app/oauth/authorize` — the browser-facing half of the MCP authorization
 * flow (see CLAUDE.md and `api/src/oauth.rs`). Reads the OAuth authorization
 * request out of the query string and shows a consent screen.
 *
 * Routed inside `AuthenticatedSession` but outside `AppShell`: it's a one-off
 * consent screen, not part of the app chrome, yet a logged-out visit still shows
 * the ordinary login page first and comes back to this same URL (query string
 * intact) once login succeeds.
 */
export default function OAuthAuthorize() {
  const [search] = useSearchParams();
  const params = parseParams(search);

  // Clickjacking guard: the site sends no frame-blocking headers, so refuse to
  // show an approve button inside someone else's frame.
  if (window.top !== window.self) {
    return (
      <Notice title="Open in a new tab">
        For your security, this page can't be shown inside another site. Open it
        directly in your browser to continue.
      </Notice>
    );
  }

  if (!params) {
    return (
      <Notice title="Invalid request">
        This link is missing required parameters, or isn't an authorization
        request Toolbox recognizes. Go back to the app or tool you started this
        from and try again.
      </Notice>
    );
  }

  return (
    <RelayErrorBoundary canRetry>
      <Suspense fallback={<LoadingIndicator />}>
        <ConsentScreen {...params} />
      </Suspense>
    </RelayErrorBoundary>
  );
}

function ConsentScreen({
  clientId,
  redirectUri,
  codeChallenge,
  codeChallengeMethod,
  scope,
  resource,
  state,
}: AuthorizationParams) {
  const user = useCurrentUser();
  const [denying, setDenying] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const data = useRetryableLazyLoadQuery<OAuthAuthorizeQuery>(
    graphql`
      query OAuthAuthorizeQuery($clientId: String!, $redirectUri: String!)
      @throwOnFieldError {
        oauthAuthorizationRequest(
          clientId: $clientId
          redirectUri: $redirectUri
        ) {
          clientName
          redirectHost
        }
      }
    `,
    { clientId, redirectUri },
  );

  const [commitMutation, isApproving] = useMutation<OAuthAuthorizeMutation>(
    graphql`
      mutation OAuthAuthorizeMutation(
        $clientId: String!
        $redirectUri: String!
        $codeChallenge: String!
        $codeChallengeMethod: String!
        $scope: String
        $resource: String
        $state: String
      ) {
        approveOauthAuthorization(
          clientId: $clientId
          redirectUri: $redirectUri
          codeChallenge: $codeChallenge
          codeChallengeMethod: $codeChallengeMethod
          scope: $scope
          resource: $resource
          state: $state
        )
      }
    `,
  );

  function handleApprove() {
    setError(null);
    commitMutation({
      variables: {
        clientId,
        redirectUri,
        codeChallenge,
        codeChallengeMethod,
        scope,
        resource,
        state,
      },
      onCompleted: (response) => {
        window.location.assign(response.approveOauthAuthorization);
      },
      onError: (err) => {
        setError(
          relayMutationErrorMessage(err, "Couldn't approve this request"),
        );
      },
    });
  }

  function handleDeny() {
    setDenying(true);
    window.location.assign(denialUrl(redirectUri, state));
  }

  const { clientName, redirectHost } = data.oauthAuthorizationRequest;
  const busy = isApproving || denying;

  return (
    <Card className="mx-auto mt-16 max-w-lg">
      <h1 className="text-xl font-semibold text-ink-strong">
        Connect {clientName}?
      </h1>
      <p className="mt-3 text-ink-muted">
        <strong className="text-ink-strong">{clientName}</strong> wants to
        connect to your Toolbox account.
      </p>
      <p
        role="note"
        className="mt-4 rounded-md border border-amber-500/50 bg-amber-500/10 p-3 text-sm text-ink"
      >
        It will redirect to <strong>{redirectHost}</strong> once approved. Only
        approve this if you started this connection yourself, from that app or
        AI tool — don't approve it because a link or message told you to.
      </p>
      <p className="mt-4 text-ink">
        This will let it act as you (<strong>{user.email}</strong>) in Toolbox,
        with all of your permissions.
      </p>
      {error && (
        <p role="alert" className="mt-4 text-sm text-red-600 dark:text-red-400">
          {error}
        </p>
      )}
      <div className="mt-6 flex flex-row flex-wrap items-center gap-3">
        <Button variant="primary" onClick={handleApprove} disabled={busy}>
          {isApproving ? "Approving…" : "Approve"}
        </Button>
        <Button variant="secondary" onClick={handleDeny} disabled={busy}>
          Deny
        </Button>
      </div>
    </Card>
  );
}
