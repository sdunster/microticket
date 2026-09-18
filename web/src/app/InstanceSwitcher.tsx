import { useEffect, useMemo } from "react";
import { graphql, useFragment } from "react-relay";
import type { InstanceSwitcher_user$key } from "./__generated__/InstanceSwitcher_user.graphql";
import {
  getStoredSelectedInstanceId,
  setStoredSelectedInstanceId,
  type SelectedInstance,
} from "./selectedInstance";

const instanceSwitcherFragment = graphql`
  fragment InstanceSwitcher_user on User {
    memberships {
      role
      instance {
        id
        name
        slug
      }
    }
  }
`;

/**
 * Which instance the signed-in user is currently working. Reads
 * `me.memberships` (via its own colocated fragment) and persists the choice
 * to `localStorage` so a reload keeps it. Calls `onChange` whenever the
 * effective selection changes — including the very first render, once
 * memberships are known — rather than owning the selected value itself, so
 * `AppShell` can thread it to nested routes via `SelectedInstanceContext`.
 */
export function InstanceSwitcher({
  user,
  selected,
  onChange,
}: {
  user: InstanceSwitcher_user$key;
  selected: SelectedInstance | null;
  onChange: (instance: SelectedInstance) => void;
}) {
  const data = useFragment(instanceSwitcherFragment, user);
  const memberships = data.memberships;

  const initialInstanceId = useMemo(() => {
    const stored = getStoredSelectedInstanceId();
    if (stored && memberships.some((m) => m.instance.id === stored)) {
      return stored;
    }
    return memberships[0]?.instance.id ?? null;
  }, [memberships]);

  useEffect(() => {
    if (selected || !initialInstanceId) return;
    const match = memberships.find((m) => m.instance.id === initialInstanceId);
    if (match) {
      const { id, name, slug } = match.instance;
      onChange({ id, name, slug });
    }
  }, [initialInstanceId, memberships, selected, onChange]);

  if (memberships.length === 0) {
    return <p className="text-sm text-ink-muted">No instances</p>;
  }

  function handleChange(instanceId: string) {
    const match = memberships.find((m) => m.instance.id === instanceId);
    if (!match) return;
    setStoredSelectedInstanceId(instanceId);
    const { id, name, slug } = match.instance;
    onChange({ id, name, slug });
  }

  const selectedRole = memberships.find(
    (m) => m.instance.id === selected?.id,
  )?.role;

  return (
    <label className="flex items-center gap-2 text-sm">
      <span className="sr-only">Instance</span>
      <select
        aria-label="Instance"
        className="rounded-md border border-line bg-surface px-2 py-1.5 text-sm text-ink"
        value={selected?.id ?? ""}
        onChange={(e) => handleChange(e.target.value)}
      >
        {memberships.map((m) => (
          <option key={m.instance.id} value={m.instance.id}>
            {m.instance.name}
          </option>
        ))}
      </select>
      {selectedRole && (
        <span className="rounded-full bg-surface-sunken px-2 py-0.5 text-xs text-ink-muted capitalize">
          {selectedRole.toLowerCase()}
        </span>
      )}
    </label>
  );
}
