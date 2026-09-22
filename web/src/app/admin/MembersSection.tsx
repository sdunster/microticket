import { useState } from "react";
import { graphql, useFragment, useMutation } from "react-relay";
import type { MembersSection_instance$key } from "./__generated__/MembersSection_instance.graphql";
import type { MembersSection_query$key } from "./__generated__/MembersSection_query.graphql";
import type { MembersSectionAddMutation } from "./__generated__/MembersSectionAddMutation.graphql";
import type {
  MembersSectionSetRoleMutation,
  MembershipRoleType,
} from "./__generated__/MembersSectionSetRoleMutation.graphql";
import type { MembersSectionRemoveMutation } from "./__generated__/MembersSectionRemoveMutation.graphql";
import { relayMutationErrorMessage } from "../../lib/relayMutationError";
import { Card } from "../../components/ui/Card";
import { Button } from "../../components/ui/Button";

const membersSectionInstanceFragment = graphql`
  fragment MembersSection_instance on Instance {
    id
    members {
      role
      user {
        id
        name
        email
      }
    }
  }
`;

// `adminUsers` — the whole user directory — is what the "add member" picker
// below chooses from, filtered down to users who aren't already members.
// Selected on `QueryRoot` (spread from `InstanceDetailPageQuery`) rather
// than fetched separately: the picker needs the full list regardless of
// which instance is open, so there's no per-instance variable to key a
// second query on.
const membersSectionQueryFragment = graphql`
  fragment MembersSection_query on QueryRoot {
    adminUsers {
      id
      name
      email
    }
  }
`;

/**
 * Members: list with an inline role select and remove, plus an add-member
 * form. `addMember`/`removeMember`/`setMemberRole` all return the instance's
 * full, fresh `members` list, so — unlike `InboundAddressesSection` — Relay
 * merges each result straight onto the `Instance` record by id with no
 * manual updater; `members` is a plain list field, and the response simply
 * replaces it wholesale.
 */
export function MembersSection({
  instance,
  query,
}: {
  instance: MembersSection_instance$key;
  query: MembersSection_query$key;
}) {
  const data = useFragment(membersSectionInstanceFragment, instance);
  const usersData = useFragment(membersSectionQueryFragment, query);
  const [error, setError] = useState<string | null>(null);
  const [newUserId, setNewUserId] = useState("");
  const [newRole, setNewRole] = useState<MembershipRoleType>("AGENT");

  const [setRole, settingRole] = useMutation<MembersSectionSetRoleMutation>(
    graphql`
      mutation MembersSectionSetRoleMutation(
        $instanceId: ID!
        $userId: ID!
        $role: MembershipRoleType!
      ) {
        setMemberRole(instanceId: $instanceId, userId: $userId, role: $role) {
          id
          members {
            role
            user {
              id
              name
              email
            }
          }
        }
      }
    `,
  );

  const [removeMember, removing] = useMutation<MembersSectionRemoveMutation>(
    graphql`
      mutation MembersSectionRemoveMutation($instanceId: ID!, $userId: ID!) {
        removeMember(instanceId: $instanceId, userId: $userId) {
          id
          members {
            role
            user {
              id
              name
              email
            }
          }
        }
      }
    `,
  );

  const [addMember, adding] = useMutation<MembersSectionAddMutation>(graphql`
    mutation MembersSectionAddMutation(
      $instanceId: ID!
      $userId: ID!
      $role: MembershipRoleType!
    ) {
      addMember(instanceId: $instanceId, userId: $userId, role: $role) {
        id
        members {
          role
          user {
            id
            name
            email
          }
        }
      }
    }
  `);

  const busy = settingRole || removing || adding;
  const existingIds = new Set(data.members.map((m) => m.user.id));
  const availableUsers = usersData.adminUsers.filter(
    (u) => !existingIds.has(u.id),
  );

  function handleRoleChange(userId: string, role: MembershipRoleType) {
    setError(null);
    setRole({
      variables: { instanceId: data.id, userId, role },
      onError: () => setError("Failed to change role."),
    });
  }

  function handleRemove(userId: string, name: string) {
    if (!window.confirm(`Remove ${name} from this instance?`)) return;
    setError(null);
    removeMember({
      variables: { instanceId: data.id, userId },
      onError: () => setError("Failed to remove member."),
    });
  }

  function handleAdd(e: React.FormEvent) {
    e.preventDefault();
    if (!newUserId || busy) return;
    setError(null);
    addMember({
      variables: { instanceId: data.id, userId: newUserId, role: newRole },
      onCompleted: () => setNewUserId(""),
      onError: (err) =>
        setError(relayMutationErrorMessage(err, "Failed to add member.")),
    });
  }

  return (
    <Card>
      <h2 className="mb-4 text-sm font-semibold tracking-wide text-ink-muted uppercase">
        Members
      </h2>

      {data.members.length === 0 ? (
        <p className="text-sm text-ink-muted">No members yet.</p>
      ) : (
        <ul className="flex flex-col divide-y divide-line-faint">
          {data.members.map((m) => (
            <li
              key={m.user.id}
              className="flex flex-wrap items-center justify-between gap-2 py-2.5"
            >
              <div>
                <p className="text-sm font-medium text-ink">{m.user.name}</p>
                <p className="text-xs text-ink-muted">{m.user.email}</p>
              </div>
              <div className="flex items-center gap-2">
                <select
                  aria-label={`Role for ${m.user.name}`}
                  className="rounded-md border border-line bg-surface px-2 py-1 text-sm text-ink"
                  value={m.role}
                  disabled={busy}
                  onChange={(e) =>
                    handleRoleChange(
                      m.user.id,
                      e.target.value as MembershipRoleType,
                    )
                  }
                >
                  <option value="OWNER">Owner</option>
                  <option value="AGENT">Agent</option>
                </select>
                <Button
                  variant="danger"
                  disabled={busy}
                  onClick={() => handleRemove(m.user.id, m.user.name)}
                >
                  Remove
                </Button>
              </div>
            </li>
          ))}
        </ul>
      )}

      <form
        onSubmit={handleAdd}
        className="mt-4 flex flex-wrap items-end gap-2 border-t border-line-faint pt-4"
      >
        <label className="flex flex-col gap-1.5 text-sm">
          <span className="font-medium text-ink">Add member</span>
          <select
            aria-label="User to add"
            className="rounded-md border border-line bg-surface px-2 py-1.5 text-sm text-ink"
            value={newUserId}
            disabled={busy}
            onChange={(e) => setNewUserId(e.target.value)}
          >
            <option value="">Choose a user…</option>
            {availableUsers.map((u) => (
              <option key={u.id} value={u.id}>
                {u.name} ({u.email})
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1.5 text-sm">
          <span className="font-medium text-ink">Role</span>
          <select
            aria-label="Role for new member"
            className="rounded-md border border-line bg-surface px-2 py-1.5 text-sm text-ink"
            value={newRole}
            disabled={busy}
            onChange={(e) => setNewRole(e.target.value as MembershipRoleType)}
          >
            <option value="AGENT">Agent</option>
            <option value="OWNER">Owner</option>
          </select>
        </label>
        <Button type="submit" variant="secondary" disabled={!newUserId || busy}>
          Add
        </Button>
      </form>

      {error && (
        <p role="alert" className="mt-3 text-sm text-red-600 dark:text-red-400">
          {error}
        </p>
      )}
    </Card>
  );
}
