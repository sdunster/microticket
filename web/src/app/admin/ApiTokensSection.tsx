import { useState } from "react";
import { graphql, useFragment, useMutation } from "react-relay";
import type { RecordSourceSelectorProxy } from "relay-runtime";
import type { ApiTokensSection_instance$key } from "./__generated__/ApiTokensSection_instance.graphql";
import type { ApiTokensSectionCreateMutation } from "./__generated__/ApiTokensSectionCreateMutation.graphql";
import type { ApiTokensSectionUpdateMutation } from "./__generated__/ApiTokensSectionUpdateMutation.graphql";
import type { ApiTokensSectionDeleteMutation } from "./__generated__/ApiTokensSectionDeleteMutation.graphql";
import { relayMutationErrorMessage } from "../../lib/relayMutationError";
import { formatRelativeTime } from "../../lib/relativeTime";
import { Card } from "../../components/ui/Card";
import { Button } from "../../components/ui/Button";
import TextInput from "../../components/ui/TextInput";

const apiTokensSectionFragment = graphql`
  fragment ApiTokensSection_instance on Instance {
    id
    apiTokens {
      id
      name
      enabled
      createdAt
      lastUsedAt
      createdBy {
        id
        name
      }
    }
  }
`;

/**
 * Instance-scoped `mta_` integration tokens that authorise exactly
 * `submitVerifiedTicket` — see `CLAUDE.md`'s "API tokens" house rule.
 * Reaching this section at all already required owner-or-superuser, same as
 * `InboundAddressesSection`/`MembersSection` on this same page.
 *
 * `createApiToken` returns `CreatedApiToken { token, apiToken }`, not the
 * instance, so — like `InboundAddressesSection` — the new row is appended
 * via an explicit updater rather than a whole-object merge; `deleteApiToken`
 * needs the same treatment to remove a row. `updateApiToken` (rename/
 * enable/disable) returns `ApiTokenInfo!` directly and *does* have an `id`,
 * so that one mutation lets Relay merge the result onto the existing record
 * with no updater at all, the same as `MembersSection`'s mutations.
 */
export function ApiTokensSection({
  instance,
}: {
  instance: ApiTokensSection_instance$key;
}) {
  const data = useFragment(apiTokensSectionFragment, instance);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  // The one and only time a freshly minted secret is ever visible — never
  // refetched, never stored anywhere but this component's own state, and
  // gone the moment the admin dismisses it or navigates away.
  const [revealedToken, setRevealedToken] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [drafts, setDrafts] = useState<Record<string, string>>({});

  const [createToken, creating] = useMutation<ApiTokensSectionCreateMutation>(
    graphql`
      mutation ApiTokensSectionCreateMutation(
        $instanceId: ID!
        $name: String!
      ) {
        createApiToken(instanceId: $instanceId, name: $name) {
          token
          apiToken {
            id
            name
            enabled
            createdAt
            lastUsedAt
            createdBy {
              id
              name
            }
          }
        }
      }
    `,
  );

  const [updateToken, updating] = useMutation<ApiTokensSectionUpdateMutation>(
    graphql`
      mutation ApiTokensSectionUpdateMutation(
        $id: ID!
        $name: String!
        $enabled: Boolean!
      ) {
        updateApiToken(id: $id, name: $name, enabled: $enabled) {
          id
          name
          enabled
          createdAt
          lastUsedAt
          createdBy {
            id
            name
          }
        }
      }
    `,
  );

  const [deleteToken, deleting] = useMutation<ApiTokensSectionDeleteMutation>(
    graphql`
      mutation ApiTokensSectionDeleteMutation($id: ID!) {
        deleteApiToken(id: $id)
      }
    `,
  );

  const busy = creating || updating || deleting;

  function handleCreate(e: React.FormEvent) {
    e.preventDefault();
    const value = name.trim();
    if (!value || creating) return;
    setError(null);

    const updater = (store: RecordSourceSelectorProxy) => {
      const instanceRecord = store.get(data.id);
      const created = store.getRootField("createApiToken");
      const newToken = created?.getLinkedRecord("apiToken");
      if (!instanceRecord || !newToken) return;
      const existing = instanceRecord.getLinkedRecords("apiTokens") ?? [];
      instanceRecord.setLinkedRecords([...existing, newToken], "apiTokens");
    };

    createToken({
      variables: { instanceId: data.id, name: value },
      updater,
      onCompleted: (response) => {
        setName("");
        setRevealedToken(response.createApiToken.token);
        setCopied(false);
      },
      onError: (err) =>
        setError(relayMutationErrorMessage(err, "Failed to create token.")),
    });
  }

  function handleCopy() {
    if (!revealedToken) return;
    navigator.clipboard
      ?.writeText(revealedToken)
      .then(() => setCopied(true))
      .catch(() => {
        // Clipboard access can be denied (permissions, non-secure context,
        // an environment with no clipboard at all) — the token is still
        // shown selectable/read-only, so this is a convenience, not the
        // only way to get it.
      });
  }

  function draftName(id: string, fallback: string) {
    return drafts[id] ?? fallback;
  }

  function handleSaveName(id: string, enabled: boolean) {
    const value = drafts[id]?.trim();
    if (!value) return;
    setError(null);
    updateToken({
      variables: { id, name: value, enabled },
      onError: () => setError("Failed to rename token."),
    });
  }

  function handleToggleEnabled(
    id: string,
    currentName: string,
    enabled: boolean,
  ) {
    setError(null);
    const name = draftName(id, currentName).trim() || currentName;
    updateToken({
      variables: { id, name, enabled },
      onError: () => setError("Failed to update token."),
    });
  }

  function handleDelete(id: string, tokenName: string) {
    if (
      !window.confirm(
        `Delete the token "${tokenName}"? Anything still using it will immediately stop working.`,
      )
    ) {
      return;
    }
    setError(null);

    const updater = (store: RecordSourceSelectorProxy) => {
      const instanceRecord = store.get(data.id);
      if (!instanceRecord) return;
      const existing = instanceRecord.getLinkedRecords("apiTokens") ?? [];
      instanceRecord.setLinkedRecords(
        existing.filter((r) => r?.getDataID() !== id),
        "apiTokens",
      );
    };

    deleteToken({
      variables: { id },
      optimisticResponse: { deleteApiToken: true },
      updater,
      onError: () => setError("Failed to delete token."),
    });
  }

  return (
    <Card>
      <h2 className="mb-1 text-sm font-semibold tracking-wide text-ink-muted uppercase">
        API tokens
      </h2>
      <p className="mb-4 text-sm text-ink-muted">
        Long-lived integration credentials that can only open tickets via{" "}
        <code>submitVerifiedTicket</code> — nothing else in the API.
      </p>

      {revealedToken && (
        <div className="mb-4 rounded-md border border-accent/40 bg-accent/5 p-3">
          <p className="mb-2 text-sm font-medium text-ink">
            Copy this token now — you won&apos;t see it again.
          </p>
          <div className="flex gap-2">
            <TextInput
              readOnly
              value={revealedToken}
              onFocus={(e) => e.currentTarget.select()}
              className="font-mono text-xs"
              aria-label="New API token secret"
            />
            <Button type="button" variant="secondary" onClick={handleCopy}>
              {copied ? "Copied" : "Copy"}
            </Button>
            <Button
              type="button"
              variant="ghost"
              onClick={() => setRevealedToken(null)}
            >
              Dismiss
            </Button>
          </div>
        </div>
      )}

      {data.apiTokens.length === 0 ? (
        <p className="text-sm text-ink-muted">No API tokens yet.</p>
      ) : (
        <ul className="flex flex-col divide-y divide-line-faint">
          {data.apiTokens.map((t) => {
            const draft = draftName(t.id, t.name);
            const dirty = draft.trim() !== t.name && draft.trim().length > 0;
            return (
              <li key={t.id} className="flex flex-col gap-2 py-2.5">
                <div className="flex flex-wrap items-center gap-2">
                  <TextInput
                    aria-label={`Name for token ${t.name}`}
                    value={draft}
                    disabled={busy}
                    onChange={(e) =>
                      setDrafts((prev) => ({ ...prev, [t.id]: e.target.value }))
                    }
                    className="max-w-xs"
                  />
                  {dirty && (
                    <Button
                      type="button"
                      variant="secondary"
                      disabled={busy}
                      onClick={() => handleSaveName(t.id, t.enabled)}
                    >
                      Save
                    </Button>
                  )}
                  <label className="ml-auto flex items-center gap-2 text-sm text-ink">
                    <input
                      type="checkbox"
                      checked={t.enabled}
                      disabled={busy}
                      onChange={(e) =>
                        handleToggleEnabled(t.id, t.name, e.target.checked)
                      }
                      className="size-4 rounded-sm border-line text-accent focus:ring-2 focus:ring-accent/25"
                    />
                    Enabled
                  </label>
                  <Button
                    variant="danger"
                    disabled={busy}
                    onClick={() => handleDelete(t.id, t.name)}
                  >
                    Delete
                  </Button>
                </div>
                <p className="text-xs text-ink-muted">
                  Created {formatRelativeTime(t.createdAt)}
                  {t.createdBy ? ` by ${t.createdBy.name}` : ""} · Last used{" "}
                  {t.lastUsedAt ? formatRelativeTime(t.lastUsedAt) : "never"}
                </p>
              </li>
            );
          })}
        </ul>
      )}

      <form
        onSubmit={handleCreate}
        className="mt-4 flex gap-2 border-t border-line-faint pt-4"
      >
        <TextInput
          aria-label="New token name"
          placeholder="e.g. Partner portal"
          value={name}
          onChange={(e) => setName(e.target.value)}
          disabled={creating}
        />
        <Button
          type="submit"
          variant="secondary"
          disabled={!name.trim() || creating}
        >
          Create token
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
