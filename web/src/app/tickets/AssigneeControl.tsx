import { useState } from "react";
import { graphql, useFragment, useMutation } from "react-relay";
import type { AssigneeControl_ticket$key } from "./__generated__/AssigneeControl_ticket.graphql";
import type { AssigneeControlMutation } from "./__generated__/AssigneeControlMutation.graphql";
import { useCurrentUser } from "../../auth/useCurrentUser";
import { Button } from "../../components/ui/Button";

const assigneeControlFragment = graphql`
  fragment AssigneeControl_ticket on Ticket
  @argumentDefinitions(isOwner: { type: "Boolean!" }) {
    id
    assigneeUserId
    assignee {
      id
      name
      email
    }
    instance {
      id
      members @include(if: $isOwner) {
        user {
          id
          name
          email
        }
      }
    }
  }
`;

/**
 * Who owns this ticket. `Instance.members` is owner-only in the schema (see
 * its doc comment), so a non-owner agent has no way to *list* teammates to
 * assign to — the field is simply never requested for them (`@include(if:
 * $isOwner)`), and they get a narrower "assign to me / unassign" control
 * instead of a full picker. This is a schema-shaped limitation, not a
 * corner we cut: widening it would mean either querying an owner-only field
 * as a non-owner (which the API correctly refuses) or adding a new,
 * non-owner-gated "assignable members" field to the schema, which is a
 * backend change out of scope here. See the step 8b report.
 */
export function AssigneeControl({
  ticket,
}: {
  ticket: AssigneeControl_ticket$key;
}) {
  const data = useFragment(assigneeControlFragment, ticket);
  const currentUser = useCurrentUser();
  const [error, setError] = useState<string | null>(null);
  const [commit, isInFlight] = useMutation<AssigneeControlMutation>(graphql`
    mutation AssigneeControlMutation($ticketId: ID!, $userId: ID) {
      assignTicket(ticketId: $ticketId, userId: $userId) {
        id
        assigneeUserId
        assignee {
          id
          name
          email
        }
      }
    }
  `);

  function assignTo(userId: string | null, name: string | null) {
    setError(null);
    commit({
      variables: { ticketId: data.id, userId },
      optimisticResponse: {
        assignTicket: {
          id: data.id,
          assigneeUserId: userId,
          assignee: userId ? { id: userId, name: name ?? "", email: "" } : null,
        },
      },
      onError: () => setError("Failed to update assignment."),
    });
  }

  const members = data.instance.members;

  return (
    <div className="flex flex-col items-end gap-1.5">
      <div className="flex items-center gap-2 text-sm">
        <span className="text-ink-muted">Assignee:</span>
        {members ? (
          <select
            aria-label="Assignee"
            className="rounded-md border border-line bg-surface px-2 py-1 text-sm text-ink"
            value={data.assigneeUserId ?? ""}
            disabled={isInFlight}
            onChange={(e) => {
              const userId = e.target.value || null;
              const member = members.find((m) => m.user.id === userId);
              assignTo(userId, member?.user.name ?? null);
            }}
          >
            <option value="">Unassigned</option>
            {members.map((m) => (
              <option key={m.user.id} value={m.user.id} title={m.user.email}>
                {m.user.name}
              </option>
            ))}
          </select>
        ) : (
          <>
            <span className="font-medium text-ink">
              {data.assignee ? data.assignee.name : "Unassigned"}
            </span>
            {data.assigneeUserId === currentUser.id ? (
              <Button
                variant="secondary"
                disabled={isInFlight}
                onClick={() => assignTo(null, null)}
              >
                Unassign
              </Button>
            ) : (
              <Button
                variant="secondary"
                disabled={isInFlight}
                onClick={() => assignTo(currentUser.id, currentUser.name)}
              >
                Assign to me
              </Button>
            )}
          </>
        )}
      </div>
      {error && (
        <p role="alert" className="text-sm text-red-600 dark:text-red-400">
          {error}
        </p>
      )}
    </div>
  );
}
