import { useState } from "react";
import { graphql, useFragment, useMutation } from "react-relay";
import type { RecordSourceSelectorProxy } from "relay-runtime";
import type { PasskeyList_user$key } from "./__generated__/PasskeyList_user.graphql";
import type { PasskeyList_RenameMutation } from "./__generated__/PasskeyList_RenameMutation.graphql";
import type { PasskeyList_DeleteMutation } from "./__generated__/PasskeyList_DeleteMutation.graphql";
import { usePasskeyRegistration } from "../auth/usePasskeyRegistration";
import { Button } from "../components/ui/Button";
import TextInput from "../components/ui/TextInput";

const passkeyListFragment = graphql`
  fragment PasskeyList_user on User {
    id
    passkeys {
      id
      name
      createdAt
      lastUsedAt
    }
  }
`;

/**
 * Passkey management: list, rename, delete, register. Every mutation here
 * uses an updater or a plain id-keyed merge — never `store.invalidateStore()`
 * — so one passkey changing doesn't refetch everything else `/app` has
 * cached (the instance switcher's memberships, in particular).
 */
export function PasskeyList({ user }: { user: PasskeyList_user$key }) {
  const data = useFragment(passkeyListFragment, user);
  const register = usePasskeyRegistration();

  const [commitRename] = useMutation<PasskeyList_RenameMutation>(graphql`
    mutation PasskeyList_RenameMutation($id: String!, $name: String!) {
      renamePasskey(id: $id, name: $name) {
        id
        name
        createdAt
        lastUsedAt
      }
    }
  `);

  const [commitDelete] = useMutation<PasskeyList_DeleteMutation>(graphql`
    mutation PasskeyList_DeleteMutation($id: String!) {
      deletePasskey(id: $id)
    }
  `);

  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState("");
  const [registerName, setRegisterName] = useState("");
  const [registering, setRegistering] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function startRename(id: string, currentName: string) {
    setEditingId(id);
    setEditingName(currentName);
    setError(null);
  }

  function submitRename(id: string) {
    const passkey = data.passkeys.find((p) => p.id === id);
    const name = editingName.trim();
    if (!passkey || !name) return;
    setEditingId(null);
    commitRename({
      variables: { id, name },
      // The rest of the row's fields don't change; carrying them over keeps
      // the optimistic record complete instead of momentarily blanking them.
      optimisticResponse: {
        renamePasskey: {
          id: passkey.id,
          name,
          createdAt: passkey.createdAt,
          lastUsedAt: passkey.lastUsedAt ?? null,
        },
      },
      onError: () => setError("Failed to rename passkey."),
    });
  }

  function removePasskey(id: string) {
    if (!window.confirm("Remove this passkey?")) return;
    const updater = (store: RecordSourceSelectorProxy) => {
      const me = store.getRoot().getLinkedRecord("me");
      if (!me) return;
      const existing = me.getLinkedRecords("passkeys") ?? [];
      me.setLinkedRecords(
        existing.filter((p) => p?.getValue("id") !== id),
        "passkeys",
      );
    };
    commitDelete({
      variables: { id },
      optimisticResponse: { deletePasskey: true },
      optimisticUpdater: updater,
      updater,
      onError: () => setError("Failed to remove passkey."),
    });
  }

  async function handleRegister(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setRegistering(true);
    try {
      await register(registerName.trim() || "New passkey");
      setRegisterName("");
    } catch (err) {
      console.warn("[passkey] registration failed:", err);
      setError("Failed to register passkey.");
    } finally {
      setRegistering(false);
    }
  }

  return (
    <div className="flex flex-col gap-4">
      {error && (
        <p role="alert" className="text-sm text-red-600 dark:text-red-400">
          {error}
        </p>
      )}

      {data.passkeys.length === 0 ? (
        <p className="text-sm text-ink-muted">No passkeys registered yet.</p>
      ) : (
        <ul className="flex flex-col divide-y divide-line-faint">
          {data.passkeys.map((passkey) => (
            <li
              key={passkey.id}
              className="flex flex-wrap items-center justify-between gap-2 py-3"
            >
              {editingId === passkey.id ? (
                <form
                  className="flex flex-1 items-center gap-2"
                  onSubmit={(e) => {
                    e.preventDefault();
                    submitRename(passkey.id);
                  }}
                >
                  <TextInput
                    aria-label="Passkey name"
                    value={editingName}
                    onChange={(e) => setEditingName(e.target.value)}
                    autoFocus
                  />
                  <Button type="submit" size="normal">
                    Save
                  </Button>
                  <Button
                    type="button"
                    variant="secondary"
                    onClick={() => setEditingId(null)}
                  >
                    Cancel
                  </Button>
                </form>
              ) : (
                <>
                  <span className="text-sm text-ink">{passkey.name}</span>
                  <div className="flex gap-2">
                    <Button
                      variant="secondary"
                      onClick={() => startRename(passkey.id, passkey.name)}
                    >
                      Rename
                    </Button>
                    <Button
                      variant="danger"
                      onClick={() => removePasskey(passkey.id)}
                    >
                      Delete
                    </Button>
                  </div>
                </>
              )}
            </li>
          ))}
        </ul>
      )}

      <form
        onSubmit={handleRegister}
        className="flex flex-wrap items-end gap-2 border-t border-line-faint pt-4"
      >
        <label className="flex flex-col gap-1.5 text-sm">
          <span className="font-medium text-ink">New passkey name</span>
          <TextInput
            value={registerName}
            onChange={(e) => setRegisterName(e.target.value)}
            placeholder="e.g. Work laptop"
          />
        </label>
        <Button type="submit" disabled={registering}>
          {registering ? "Registering…" : "Register a passkey"}
        </Button>
      </form>
    </div>
  );
}
