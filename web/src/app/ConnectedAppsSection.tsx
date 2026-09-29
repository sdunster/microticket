import { useState } from "react";
import { graphql, useFragment, useMutation } from "react-relay";
import type { RecordSourceSelectorProxy } from "relay-runtime";
import type { ConnectedAppsSection_user$key } from "./__generated__/ConnectedAppsSection_user.graphql";
import type { ConnectedAppsSection_RevokeMutation } from "./__generated__/ConnectedAppsSection_RevokeMutation.graphql";
import { formatRelativeTime } from "../lib/relativeTime";
import { relayMutationErrorMessage } from "../lib/relayMutationError";
import { Button } from "../components/ui/Button";

const connectedAppsFragment = graphql`
  fragment ConnectedAppsSection_user on User {
    id
    oauthGrants {
      id
      clientName
      redirectHost
      createdAt
      lastUsedAt
      refreshExpiresAt
    }
  }
`;

/**
 * "Connected AI apps": the MCP clients this user has authorized to act as
 * them, with a Disconnect button per app. Revoking removes the row from the
 * store with an updater rather than refetching, same as `PasskeyList`.
 */
export function ConnectedAppsSection({
  user,
}: {
  user: ConnectedAppsSection_user$key;
}) {
  const data = useFragment(connectedAppsFragment, user);
  const [error, setError] = useState<string | null>(null);

  const [commitRevoke, isRevoking] =
    useMutation<ConnectedAppsSection_RevokeMutation>(graphql`
      mutation ConnectedAppsSection_RevokeMutation($id: ID!) {
        revokeOauthGrant(id: $id)
      }
    `);

  function disconnect(id: string, clientName: string) {
    if (
      !window.confirm(
        `Disconnect "${clientName}"? It will need to be reconnected to act as you again.`,
      )
    ) {
      return;
    }
    setError(null);
    commitRevoke({
      variables: { id },
      updater: (store: RecordSourceSelectorProxy) => {
        const me = store.getRoot().getLinkedRecord("me");
        if (!me) return;
        me.setLinkedRecords(
          (me.getLinkedRecords("oauthGrants") ?? []).filter(
            (g) => g?.getValue("id") !== id,
          ),
          "oauthGrants",
        );
      },
      onError: (err) =>
        setError(
          relayMutationErrorMessage(err, `Couldn't disconnect "${clientName}"`),
        ),
    });
  }

  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-ink-muted">
        AI tools you connect through Toolbox&apos;s MCP interface — for example
        Claude Code or a claude.ai custom connector — appear here once you
        approve them. Each one can act as you, with your permissions. Disconnect
        anything you no longer use or don&apos;t recognize.
      </p>
      {error && (
        <p role="alert" className="text-sm text-red-600 dark:text-red-400">
          {error}
        </p>
      )}
      {data.oauthGrants.length === 0 ? (
        <p className="text-sm text-ink-muted">No AI apps connected yet.</p>
      ) : (
        <ul className="flex flex-col divide-y divide-line-faint">
          {data.oauthGrants.map((grant) => (
            <li
              key={grant.id}
              className="flex flex-wrap items-center justify-between gap-2 py-3"
            >
              <div className="flex flex-col text-sm">
                <span className="text-ink">
                  {grant.clientName}{" "}
                  <span className="text-ink-muted">
                    (redirects to {grant.redirectHost})
                  </span>
                </span>
                <span className="text-ink-muted">
                  Connected {formatRelativeTime(grant.createdAt)} · last used{" "}
                  {grant.lastUsedAt
                    ? formatRelativeTime(grant.lastUsedAt)
                    : "never"}{" "}
                  · expires if unused{" "}
                  {new Date(grant.refreshExpiresAt * 1000).toLocaleDateString()}
                </span>
              </div>
              <Button
                variant="danger"
                disabled={isRevoking}
                onClick={() => disconnect(grant.id, grant.clientName)}
              >
                Disconnect
              </Button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
