import { Suspense } from "react";
import { graphql } from "react-relay";
import type { InstanceListPageQuery } from "./__generated__/InstanceListPageQuery.graphql";
import { useRetryableLazyLoadQuery } from "../../components/useRetryableLazyLoadQuery";
import RelayErrorBoundary from "../../components/RelayErrorBoundary";
import LoadingIndicator from "../../components/LoadingIndicator";
import { InstanceRow } from "./InstanceRow";
import { CreateInstanceForm } from "./CreateInstanceForm";

const instanceListPageQuery = graphql`
  query InstanceListPageQuery @throwOnFieldError {
    adminInstances {
      id
      ...InstanceRow_instance
    }
  }
`;

function Content() {
  const data = useRetryableLazyLoadQuery<InstanceListPageQuery>(
    instanceListPageQuery,
    {},
  );

  return (
    <div className="flex max-w-2xl flex-col gap-8">
      <div>
        <h1 className="text-xl font-semibold text-ink-strong">Instances</h1>
        <p className="mt-1 text-sm text-ink-muted">
          Every tenant, including deleted ones — a deleted instance can be
          restored from its own page.
        </p>
      </div>

      <CreateInstanceForm />

      {data.adminInstances.length === 0 ? (
        <p className="text-sm text-ink-muted">No instances yet.</p>
      ) : (
        <ul className="flex flex-col divide-y divide-line-faint rounded-lg border border-line">
          {data.adminInstances.map((instance) => (
            <li key={instance.id}>
              <InstanceRow instance={instance} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** `/app/admin/instances` — every instance, plus the create form. */
export function InstanceListPage() {
  return (
    <RelayErrorBoundary canRetry>
      <Suspense fallback={<LoadingIndicator />}>
        <Content />
      </Suspense>
    </RelayErrorBoundary>
  );
}
