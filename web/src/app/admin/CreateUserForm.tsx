import { useState } from "react";
import { graphql, useMutation } from "react-relay";
import { useNavigate } from "react-router";
import type { RecordSourceSelectorProxy } from "relay-runtime";
import type { CreateUserFormMutation } from "./__generated__/CreateUserFormMutation.graphql";
import { relayMutationErrorMessage } from "../../lib/relayMutationError";
import { Card } from "../../components/ui/Card";
import { Button } from "../../components/ui/Button";
import { FormField } from "../../components/ui/FormField";
import TextInput from "../../components/ui/TextInput";

/**
 * `createUser` grants no access by itself (see the mutation's doc comment) —
 * the note below says so, since a superuser creating a user and then
 * finding they can't log in anywhere would otherwise look like a bug.
 * `adminUsers` is a plain list field, not a `@connection`, so appending the
 * new user needs an explicit updater, same as `CreateInstanceForm`.
 */
export function CreateUserForm() {
  const navigate = useNavigate();
  const [email, setEmail] = useState("");
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);

  const [commit, isSaving] = useMutation<CreateUserFormMutation>(graphql`
    mutation CreateUserFormMutation($email: String!, $name: String!) {
      createUser(email: $email, name: $name) {
        id
        ...UserRow_user
      }
    }
  `);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (isSaving) return;
    setError(null);

    const updater = (store: RecordSourceSelectorProxy) => {
      const root = store.getRoot();
      const newUser = store.getRootField("createUser");
      if (!newUser) return;
      const existing = root.getLinkedRecords("adminUsers") ?? [];
      root.setLinkedRecords([...existing, newUser], "adminUsers");
    };

    commit({
      variables: { email: email.trim(), name: name.trim() },
      updater,
      onCompleted: (data) => {
        navigate(`/app/admin/users/${data.createUser.id}`);
      },
      onError: (err) =>
        setError(relayMutationErrorMessage(err, "Failed to create user.")),
    });
  }

  return (
    <Card>
      <h2 className="mb-4 text-sm font-semibold tracking-wide text-ink-muted uppercase">
        New user
      </h2>
      <form onSubmit={handleSubmit} className="flex flex-col gap-4">
        <FormField label="Email" htmlFor="new-user-email">
          <TextInput
            id="new-user-email"
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            required
          />
        </FormField>
        <FormField label="Name" htmlFor="new-user-name">
          <TextInput
            id="new-user-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            required
          />
        </FormField>
        <p className="text-xs text-ink-muted">
          A new user has no access to anything until they&apos;re added as a
          member of an instance.
        </p>

        {error && (
          <p role="alert" className="text-sm text-red-600 dark:text-red-400">
            {error}
          </p>
        )}

        <Button type="submit" disabled={isSaving} className="self-start">
          {isSaving ? "Creating…" : "Create user"}
        </Button>
      </form>
    </Card>
  );
}
