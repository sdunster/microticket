import { useState } from "react";
import { graphql, useMutation } from "react-relay";
import { useNavigate } from "react-router";
import type { RecordSourceSelectorProxy } from "relay-runtime";
import type { CreateInstanceFormMutation } from "./__generated__/CreateInstanceFormMutation.graphql";
import { relayMutationErrorMessage } from "../../lib/relayMutationError";
import { Card } from "../../components/ui/Card";
import { Button } from "../../components/ui/Button";
import { FormField } from "../../components/ui/FormField";
import TextInput from "../../components/ui/TextInput";

/**
 * `createInstance` is superuser-only and deliberately low-frequency — an
 * operator action, not a self-serve one (see the mutation's doc comment).
 * `adminInstances` is a plain list field, not a `@connection`, so appending
 * the new instance needs an explicit updater rather than Relay's connection
 * helpers. Navigates to the new instance's own page on success, where
 * members and inbound addresses get set up next.
 */
export function CreateInstanceForm() {
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [slug, setSlug] = useState("");
  const [fromName, setFromName] = useState("");
  const [signature, setSignature] = useState("");
  const [publicSubmissionEnabled, setPublicSubmissionEnabled] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [commit, isSaving] = useMutation<CreateInstanceFormMutation>(graphql`
    mutation CreateInstanceFormMutation(
      $name: String!
      $slug: String!
      $fromName: String
      $signature: String
      $publicSubmissionEnabled: Boolean!
    ) {
      createInstance(
        name: $name
        slug: $slug
        fromName: $fromName
        signature: $signature
        publicSubmissionEnabled: $publicSubmissionEnabled
      ) {
        id
        ...InstanceRow_instance
      }
    }
  `);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (isSaving) return;
    setError(null);

    const updater = (store: RecordSourceSelectorProxy) => {
      const root = store.getRoot();
      const newInstance = store.getRootField("createInstance");
      if (!newInstance) return;
      const existing = root.getLinkedRecords("adminInstances") ?? [];
      root.setLinkedRecords([...existing, newInstance], "adminInstances");
    };

    commit({
      variables: {
        name: name.trim(),
        slug: slug.trim(),
        fromName: fromName.trim() || null,
        signature: signature.trim() || null,
        publicSubmissionEnabled,
      },
      updater,
      onCompleted: (data) => {
        navigate(`/app/admin/instances/${data.createInstance.id}`);
      },
      onError: (err) =>
        setError(relayMutationErrorMessage(err, "Failed to create instance.")),
    });
  }

  return (
    <Card>
      <h2 className="mb-4 text-sm font-semibold tracking-wide text-ink-muted uppercase">
        New instance
      </h2>
      <form onSubmit={handleSubmit} className="flex flex-col gap-4">
        <FormField label="Name" htmlFor="new-instance-name">
          <TextInput
            id="new-instance-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            required
          />
        </FormField>
        <FormField label="Slug" htmlFor="new-instance-slug">
          <TextInput
            id="new-instance-slug"
            value={slug}
            onChange={(e) => setSlug(e.target.value)}
            pattern="[a-z0-9]+(-[a-z0-9]+)*"
            placeholder="acme-support"
            required
          />
          <p className="mt-1 text-xs text-ink-muted">
            Lowercase letters, digits, and hyphens only — and permanent once
            created, since it&apos;s stamped into the &quot;[#slug-N]&quot;
            subject tag on every ticket&apos;s mail thread.
          </p>
        </FormField>
        <FormField label="From name" htmlFor="new-instance-from-name">
          <TextInput
            id="new-instance-from-name"
            value={fromName}
            onChange={(e) => setFromName(e.target.value)}
            placeholder="Defaults to the instance name"
          />
        </FormField>
        <FormField label="Signature" htmlFor="new-instance-signature">
          <textarea
            id="new-instance-signature"
            className="w-full rounded-md border border-line bg-surface px-3 py-2 text-sm text-ink transition-colors focus:border-accent focus:ring-2 focus:ring-accent/25 focus:outline-none"
            rows={3}
            value={signature}
            onChange={(e) => setSignature(e.target.value)}
          />
        </FormField>
        <label className="flex items-center gap-2 text-sm text-ink">
          <input
            type="checkbox"
            checked={publicSubmissionEnabled}
            onChange={(e) => setPublicSubmissionEnabled(e.target.checked)}
            className="size-4 rounded-sm border-line text-accent focus:ring-2 focus:ring-accent/25"
          />
          Public submission enabled
        </label>

        {error && (
          <p role="alert" className="text-sm text-red-600 dark:text-red-400">
            {error}
          </p>
        )}

        <Button type="submit" disabled={isSaving} className="self-start">
          {isSaving ? "Creating…" : "Create instance"}
        </Button>
      </form>
    </Card>
  );
}
