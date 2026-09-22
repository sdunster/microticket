import { useState } from "react";
import { graphql, useFragment, useMutation } from "react-relay";
import type { RecordSourceSelectorProxy } from "relay-runtime";
import type { InboundAddressesSection_instance$key } from "./__generated__/InboundAddressesSection_instance.graphql";
import type { InboundAddressesSectionAddMutation } from "./__generated__/InboundAddressesSectionAddMutation.graphql";
import type { InboundAddressesSectionRemoveMutation } from "./__generated__/InboundAddressesSectionRemoveMutation.graphql";
import { relayMutationErrorMessage } from "../../lib/relayMutationError";
import { Card } from "../../components/ui/Card";
import { Button } from "../../components/ui/Button";
import TextInput from "../../components/ui/TextInput";

const inboundAddressesSectionFragment = graphql`
  fragment InboundAddressesSection_instance on Instance {
    id
    inboundAddresses {
      address
      kind
      createdAt
    }
  }
`;

/**
 * Owner-or-superuser in the API (see `Instance.inboundAddresses`'s doc
 * comment) — reaching this section at all already required superuser here.
 * `addInboundAddress`/`removeInboundAddress` return `InboundAddressInfo!`/
 * `Boolean!`, not the instance, and `InboundAddressInfo` itself has no `id`
 * field — unlike `MembersSection`'s mutations, there's no whole-object
 * merge-by-id to lean on, so both mutations use an explicit updater
 * instead, the same pattern `PasskeyList` uses for a list without ids.
 */
export function InboundAddressesSection({
  instance,
}: {
  instance: InboundAddressesSection_instance$key;
}) {
  const data = useFragment(inboundAddressesSectionFragment, instance);
  const [address, setAddress] = useState("");
  const [error, setError] = useState<string | null>(null);

  const [addAddress, adding] = useMutation<InboundAddressesSectionAddMutation>(
    graphql`
      mutation InboundAddressesSectionAddMutation(
        $instanceId: ID!
        $address: String!
      ) {
        addInboundAddress(instanceId: $instanceId, address: $address) {
          address
          kind
          createdAt
        }
      }
    `,
  );

  const [removeAddress, removing] =
    useMutation<InboundAddressesSectionRemoveMutation>(graphql`
      mutation InboundAddressesSectionRemoveMutation(
        $instanceId: ID!
        $address: String!
      ) {
        removeInboundAddress(instanceId: $instanceId, address: $address)
      }
    `);

  function handleAdd(e: React.FormEvent) {
    e.preventDefault();
    const value = address.trim();
    if (!value || adding) return;
    setError(null);

    const updater = (store: RecordSourceSelectorProxy) => {
      const instanceRecord = store.get(data.id);
      const newAddress = store.getRootField("addInboundAddress");
      if (!instanceRecord || !newAddress) return;
      const existing =
        instanceRecord.getLinkedRecords("inboundAddresses") ?? [];
      instanceRecord.setLinkedRecords(
        [...existing, newAddress],
        "inboundAddresses",
      );
    };

    addAddress({
      variables: { instanceId: data.id, address: value },
      updater,
      onCompleted: () => setAddress(""),
      onError: (err) =>
        setError(relayMutationErrorMessage(err, "Failed to add address.")),
    });
  }

  function handleRemove(addr: string) {
    if (
      !window.confirm(
        `Remove ${addr}? Mail sent to it will stop reaching this instance.`,
      )
    ) {
      return;
    }
    setError(null);

    const updater = (store: RecordSourceSelectorProxy) => {
      const instanceRecord = store.get(data.id);
      if (!instanceRecord) return;
      const existing =
        instanceRecord.getLinkedRecords("inboundAddresses") ?? [];
      instanceRecord.setLinkedRecords(
        existing.filter((r) => r?.getValue("address") !== addr),
        "inboundAddresses",
      );
    };

    removeAddress({
      variables: { instanceId: data.id, address: addr },
      optimisticResponse: { removeInboundAddress: true },
      updater,
      onError: () => setError("Failed to remove address."),
    });
  }

  return (
    <Card>
      <h2 className="mb-4 text-sm font-semibold tracking-wide text-ink-muted uppercase">
        Inbound addresses
      </h2>

      {data.inboundAddresses.length === 0 ? (
        <p className="text-sm text-ink-muted">No inbound addresses yet.</p>
      ) : (
        <ul className="flex flex-col divide-y divide-line-faint">
          {data.inboundAddresses.map((a) => (
            <li
              key={a.address}
              className="flex items-center justify-between gap-2 py-2"
            >
              <span
                className="text-sm text-ink"
                title={`Added ${new Date(a.createdAt * 1000).toLocaleString()}`}
              >
                {a.address}
              </span>
              <div className="flex items-center gap-2">
                <span className="text-xs text-ink-muted capitalize">
                  {a.kind.toLowerCase()}
                </span>
                <Button
                  variant="danger"
                  disabled={removing}
                  onClick={() => handleRemove(a.address)}
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
        className="mt-4 flex gap-2 border-t border-line-faint pt-4"
      >
        <TextInput
          aria-label="New inbound address"
          placeholder="support@example.com or *@example.com"
          value={address}
          onChange={(e) => setAddress(e.target.value)}
          disabled={adding}
        />
        <Button
          type="submit"
          variant="secondary"
          disabled={!address.trim() || adding}
        >
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
