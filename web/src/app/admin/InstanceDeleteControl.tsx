import { useState } from "react";
import { graphql, useFragment, useMutation } from "react-relay";
import type { InstanceDeleteControl_instance$key } from "./__generated__/InstanceDeleteControl_instance.graphql";
import type { InstanceDeleteControlMutation } from "./__generated__/InstanceDeleteControlMutation.graphql";
import { Card } from "../../components/ui/Card";
import { Button } from "../../components/ui/Button";

const instanceDeleteControlFragment = graphql`
  fragment InstanceDeleteControl_instance on Instance {
    id
    deleted
  }
`;

/**
 * Delete/restore — `setInstanceDeleted(deleted:)`. Soft delete only: it
 * hides the instance from every member's switcher and drops inbound mail
 * addressed to it (see that mutation's doc comment), never touches tickets
 * or members, and is fully reversible from this same page — the confirm
 * dialog says so, since "delete" reads as permanent otherwise.
 */
export function InstanceDeleteControl({
  instance,
}: {
  instance: InstanceDeleteControl_instance$key;
}) {
  const data = useFragment(instanceDeleteControlFragment, instance);
  const [error, setError] = useState<string | null>(null);

  const [commit, isInFlight] = useMutation<InstanceDeleteControlMutation>(
    graphql`
      mutation InstanceDeleteControlMutation($id: ID!, $deleted: Boolean!) {
        setInstanceDeleted(id: $id, deleted: $deleted) {
          id
          deleted
        }
      }
    `,
  );

  function setDeleted(deleted: boolean) {
    if (
      deleted &&
      !window.confirm(
        "Delete this instance? It disappears from every member's instance " +
          "switcher and inbound mail addressed to it is silently dropped. " +
          "You can restore it from this same page at any time.",
      )
    ) {
      return;
    }
    setError(null);
    commit({
      variables: { id: data.id, deleted },
      optimisticResponse: { setInstanceDeleted: { id: data.id, deleted } },
      onError: () =>
        setError(
          deleted
            ? "Failed to delete instance."
            : "Failed to restore instance.",
        ),
    });
  }

  return (
    <Card>
      <h2 className="mb-2 text-sm font-semibold tracking-wide text-ink-muted uppercase">
        {data.deleted ? "Deleted" : "Danger zone"}
      </h2>
      <p className="mb-4 text-sm text-ink-muted">
        {data.deleted
          ? "This instance is deleted: hidden from members, and inbound mail to it is dropped. Restoring undoes both immediately."
          : "Deleting hides this instance from every member and drops inbound mail addressed to it. It can be restored here at any time — nothing else is affected."}
      </p>
      {error && (
        <p role="alert" className="mb-3 text-sm text-red-600 dark:text-red-400">
          {error}
        </p>
      )}
      {data.deleted ? (
        <Button
          variant="secondary"
          disabled={isInFlight}
          onClick={() => setDeleted(false)}
        >
          Restore instance
        </Button>
      ) : (
        <Button
          variant="danger"
          disabled={isInFlight}
          onClick={() => setDeleted(true)}
        >
          Delete instance
        </Button>
      )}
    </Card>
  );
}
