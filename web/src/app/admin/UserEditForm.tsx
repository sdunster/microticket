import { useState } from "react";
import { graphql, useFragment, useMutation } from "react-relay";
import type { UserEditForm_user$key } from "./__generated__/UserEditForm_user.graphql";
import type { UserEditFormMutation } from "./__generated__/UserEditFormMutation.graphql";
import { useCurrentUser } from "../../auth/useCurrentUser";
import { relayMutationErrorMessage } from "../../lib/relayMutationError";
import { Card } from "../../components/ui/Card";
import { Button } from "../../components/ui/Button";
import { FormField } from "../../components/ui/FormField";
import TextInput from "../../components/ui/TextInput";

const userEditFormFragment = graphql`
  fragment UserEditForm_user on User {
    id
    name
    email
    enabled
    isSuperuser
  }
`;

/**
 * Name / email / enabled. `isSuperuser` is shown read-only — it's granted
 * via the CLI (`cli user set-superuser`) only, not over GraphQL at all (see
 * `User.isSuperuser`'s doc comment), so there's no mutation to wire a
 * toggle to here.
 *
 * The enabled toggle is hidden entirely, not just disabled, on the caller's
 * own record: `updateUser` rejects `enabled: false` for the caller
 * themselves (self-lockout — see that mutation's doc comment), and the API
 * is the real boundary regardless, but a control that always errors when
 * used is worse UX than one that isn't there.
 */
export function UserEditForm({ user }: { user: UserEditForm_user$key }) {
  const data = useFragment(userEditFormFragment, user);
  const currentUser = useCurrentUser();
  const isSelf = currentUser.id === data.id;

  const [name, setName] = useState(data.name);
  const [email, setEmail] = useState(data.email);
  const [enabled, setEnabled] = useState(data.enabled);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  // `enabled` can change out from under this form — `UserDeleteControl`'s
  // `deleteUser` mutation flips it to `false` in the store — so, unlike
  // `name`/`email` (which nothing else here ever writes), it has to resync
  // from the fragment rather than staying frozen at its value when this form
  // mounted. Adjusted during render (React's documented pattern for this),
  // not in an effect, which would mean an extra commit every time.
  const [prevDataEnabled, setPrevDataEnabled] = useState(data.enabled);
  if (data.enabled !== prevDataEnabled) {
    setPrevDataEnabled(data.enabled);
    setEnabled(data.enabled);
  }

  const [commit, isSaving] = useMutation<UserEditFormMutation>(graphql`
    mutation UserEditFormMutation(
      $id: ID!
      $name: String!
      $email: String!
      $enabled: Boolean!
    ) {
      updateUser(id: $id, name: $name, email: $email, enabled: $enabled) {
        id
        name
        email
        enabled
      }
    }
  `);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (isSaving) return;
    setError(null);
    setSaved(false);
    commit({
      variables: {
        id: data.id,
        name: name.trim(),
        email: email.trim(),
        enabled: isSelf ? true : enabled,
      },
      onCompleted: () => setSaved(true),
      onError: (err) =>
        setError(relayMutationErrorMessage(err, "Failed to save user.")),
    });
  }

  return (
    <Card>
      <h2 className="mb-4 text-sm font-semibold tracking-wide text-ink-muted uppercase">
        Details
      </h2>
      <form onSubmit={handleSubmit} className="flex flex-col gap-4">
        <FormField label="Name" htmlFor="user-name">
          <TextInput
            id="user-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            required
          />
        </FormField>
        <FormField label="Email" htmlFor="user-email">
          <TextInput
            id="user-email"
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            required
          />
        </FormField>

        {isSelf ? (
          <p className="text-sm text-ink-muted">
            You can&apos;t disable your own account, so there&apos;s no toggle
            here.
          </p>
        ) : (
          <label className="flex items-center gap-2 text-sm text-ink">
            <input
              type="checkbox"
              checked={enabled}
              onChange={(e) => setEnabled(e.target.checked)}
              className="size-4 rounded-sm border-line text-accent focus:ring-2 focus:ring-accent/25"
            />
            Enabled
          </label>
        )}

        <div>
          <span className="text-sm font-medium text-ink">Superuser</span>
          <p className="text-sm text-ink-muted">
            {data.isSuperuser ? "Yes" : "No"} — granted via the CLI (
            <code>cli user set-superuser</code>), not editable here.
          </p>
        </div>

        {error && (
          <p role="alert" className="text-sm text-red-600 dark:text-red-400">
            {error}
          </p>
        )}
        {saved && !error && (
          <p className="text-sm text-green-700 dark:text-green-400">Saved.</p>
        )}

        <Button type="submit" disabled={isSaving} className="self-start">
          {isSaving ? "Saving…" : "Save"}
        </Button>
      </form>
    </Card>
  );
}
