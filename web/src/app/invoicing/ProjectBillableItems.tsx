import { Suspense, useState } from "react";
import { graphql, useMutation } from "react-relay";
import type { ProjectBillableItemsQuery } from "./__generated__/ProjectBillableItemsQuery.graphql";
import type { ProjectBillableItemsCreateMutation } from "./__generated__/ProjectBillableItemsCreateMutation.graphql";
import { useRetryableLazyLoadQuery } from "../../components/useRetryableLazyLoadQuery";
import RelayErrorBoundary from "../../components/RelayErrorBoundary";
import LoadingIndicator from "../../components/LoadingIndicator";
import { Card } from "../../components/ui/Card";
import { relayMutationErrorMessage } from "../../lib/relayMutationError";
import { localToday } from "../../lib/dates";
import { BillableItemList } from "./BillableItemList";
import { BillableItemForm } from "./BillableItemForm";

const projectBillableItemsQuery = graphql`
  query ProjectBillableItemsQuery($instanceId: ID!, $projectId: ID!)
  @throwOnFieldError {
    ...BillableItemList_query
      @arguments(instanceId: $instanceId, projectId: $projectId, filter: ALL)
  }
`;

function Items({
  instanceId,
  projectId,
  currency,
  refreshKey,
}: {
  instanceId: string;
  projectId: string;
  currency: string;
  refreshKey: number;
}) {
  const data = useRetryableLazyLoadQuery<ProjectBillableItemsQuery>(
    projectBillableItemsQuery,
    { instanceId, projectId },
  );
  return (
    <BillableItemList
      query={data}
      filter="ALL"
      showProject={false}
      editable
      currency={currency}
      emptyMessage="No billable items on this project yet."
      refreshKey={refreshKey}
    />
  );
}

/**
 * "Add item" for one project. After a successful create the form remounts
 * (fresh fields) but keeps the date just used — entering a day's work item
 * by item shouldn't mean re-picking the date each time — and tells the list
 * to refetch, so the new item appears in its date-ordered place.
 */
export function AddBillableItemForm({
  projectId,
  currency,
  onCreated,
}: {
  projectId: string;
  currency: string;
  onCreated: () => void;
}) {
  const [formKey, setFormKey] = useState(0);
  const [lastDate, setLastDate] = useState(() => localToday());
  const [error, setError] = useState<string | null>(null);

  const [commit, isSaving] = useMutation<ProjectBillableItemsCreateMutation>(
    graphql`
      mutation ProjectBillableItemsCreateMutation(
        $projectId: ID!
        $input: BillableItemInput!
      ) {
        createBillableItem(projectId: $projectId, input: $input) {
          id
        }
      }
    `,
  );

  return (
    <Card>
      <h3 className="mb-4 text-sm font-semibold tracking-wide text-ink-muted uppercase">
        Add item
      </h3>
      <BillableItemForm
        key={formKey}
        idPrefix="new-item"
        currency={currency}
        initial={{ date: lastDate, description: "", quantity: "", price: "" }}
        submitLabel="Add item"
        savingLabel="Adding…"
        isSaving={isSaving}
        error={error}
        onSubmit={(input) => {
          setError(null);
          commit({
            variables: { projectId, input },
            onCompleted: () => {
              setLastDate(input.date);
              setFormKey((k) => k + 1);
              onCreated();
            },
            onError: (err) =>
              setError(relayMutationErrorMessage(err, "Failed to add item.")),
          });
        }}
      />
    </Card>
  );
}

/**
 * The project page's "Billable items" section: this project's items (with
 * inline Edit/Delete for unbilled ones) and, unless the project is
 * archived, the "Add item" form. `instanceId` is the *project's* instance,
 * not the switcher's selection — `billableItems` rejects a project from any
 * other instance, and a deep link can arrive with a different one selected.
 */
export function ProjectBillableItems({
  instanceId,
  projectId,
  currency,
  archived,
}: {
  instanceId: string;
  projectId: string;
  currency: string;
  archived: boolean;
}) {
  const [refreshKey, setRefreshKey] = useState(0);

  return (
    <section
      aria-labelledby="billable-items-heading"
      className="flex flex-col gap-4"
    >
      <h2
        id="billable-items-heading"
        className="text-lg font-semibold text-ink-strong"
      >
        Billable items
      </h2>
      <RelayErrorBoundary canRetry>
        <Suspense fallback={<LoadingIndicator />}>
          <Items
            instanceId={instanceId}
            projectId={projectId}
            currency={currency}
            refreshKey={refreshKey}
          />
        </Suspense>
      </RelayErrorBoundary>
      {archived ? (
        <p className="rounded-lg border border-dashed border-line p-4 text-sm text-ink-muted">
          This project is archived, so it can&apos;t take new items. Untick
          &quot;Archived&quot; above to add more.
        </p>
      ) : (
        <div className="max-w-3xl">
          <AddBillableItemForm
            projectId={projectId}
            currency={currency}
            onCreated={() => setRefreshKey((k) => k + 1)}
          />
        </div>
      )}
    </section>
  );
}
