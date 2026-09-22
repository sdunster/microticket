import { useState } from "react";
import { graphql, useFragment, useMutation } from "react-relay";
import type { UserDeleteControl_user$key } from "./__generated__/UserDeleteControl_user.graphql";
import type { UserDeleteControlMutation } from "./__generated__/UserDeleteControlMutation.graphql";
import { useCurrentUser } from "../../auth/useCurrentUser";
import { Card } from "../../components/ui/Card";
import { Button } from "../../components/ui/Button";

const userDeleteControlFragment = graphql`
  fragment UserDeleteControl_user on User {
    id
  }
`;

/**
 * Soft-delete: disables the account and removes every membership it holds
 * (see `deleteUser`'s doc comment) — irreversible for the memberships even
 * though `enabled` itself can be flipped back on later, which is why the
 * confirm spells that out rather than just saying "delete".
 *
 * `deleteUser` rejects the caller's own id (self-lockout, same as
 * `updateUser`'s `enabled: false`) — hidden entirely here for the same
 * reason `UserEditForm` hides its own enabled toggle for `isSelf`.
 */
export function UserDeleteControl({
  user,
}: {
  user: UserDeleteControl_user$key;
}) {
  const data = useFragment(userDeleteControlFragment, user);
  const currentUser = useCurrentUser();
  const [error, setError] = useState<string | null>(null);

  const [commit, isInFlight] = useMutation<UserDeleteControlMutation>(graphql`
    mutation UserDeleteControlMutation($id: ID!) {
      deleteUser(id: $id) {
        id
        enabled
        memberships {
          role
          instance {
            id
            name
            slug
          }
        }
      }
    }
  `);

  if (currentUser.id === data.id) {
    return (
      <Card>
        <h2 className="mb-2 text-sm font-semibold tracking-wide text-ink-muted uppercase">
          Danger zone
        </h2>
        <p className="text-sm text-ink-muted">
          You can&apos;t delete your own account.
        </p>
      </Card>
    );
  }

  function handleDelete() {
    if (
      !window.confirm(
        "Delete this user? Their account is disabled immediately (blocking " +
          "login) and every membership they hold is removed. Re-enabling " +
          "the account later does not restore those memberships.",
      )
    ) {
      return;
    }
    setError(null);
    commit({
      variables: { id: data.id },
      onError: () => setError("Failed to delete user."),
    });
  }

  return (
    <Card>
      <h2 className="mb-2 text-sm font-semibold tracking-wide text-ink-muted uppercase">
        Danger zone
      </h2>
      <p className="mb-4 text-sm text-ink-muted">
        Disables the account and removes every membership it holds. The account
        can be re-enabled from the form above, but its memberships won&apos;t
        come back — they&apos;d need to be added again.
      </p>
      {error && (
        <p role="alert" className="mb-3 text-sm text-red-600 dark:text-red-400">
          {error}
        </p>
      )}
      <Button variant="danger" disabled={isInFlight} onClick={handleDelete}>
        Delete user
      </Button>
    </Card>
  );
}
