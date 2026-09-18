import { Suspense } from "react";
import {
  graphql,
  RelayEnvironmentProvider,
  useLazyLoadQuery,
} from "react-relay";
import { Link } from "react-router";
import type { PublicInstanceListQuery } from "./__generated__/PublicInstanceListQuery.graphql";
import { unauthenticatedEnvironment } from "../lib/environments";
import { Card } from "../components/ui/Card";
import LoadingIndicator from "../components/LoadingIndicator";
import RelayErrorBoundary from "../components/RelayErrorBoundary";

const publicInstanceListQuery = graphql`
  query PublicInstanceListQuery @throwOnFieldError {
    publicInstances {
      name
      slug
    }
  }
`;

function Content() {
  // `network-only`: the shared `unauthenticatedEnvironment` singleton has no
  // per-visit lifetime to key a cache by (see its doc comment) — every
  // mount of this list should see current data, not whatever the store
  // happened to cache from an earlier visit in the same session.
  const data = useLazyLoadQuery<PublicInstanceListQuery>(
    publicInstanceListQuery,
    {},
    { fetchPolicy: "network-only" },
  );

  if (data.publicInstances.length === 0) {
    return (
      <p className="text-sm text-ink-muted">
        No organisations currently accept public ticket submissions here.
      </p>
    );
  }

  return (
    <ul className="flex flex-col gap-2">
      {data.publicInstances.map((instance) => (
        <li key={instance.slug}>
          <Link
            to={`/submit/${instance.slug}`}
            className="block rounded-md border border-line px-4 py-3 text-ink no-underline transition-colors hover:bg-surface-raised"
          >
            {instance.name}
          </Link>
        </li>
      ))}
    </ul>
  );
}

/**
 * Bare `/submit`: only instances with `publicSubmissionEnabled` — the
 * unauthenticated `publicInstances` query already filters this server-side,
 * so an instance that hasn't opted in is never even sent to the client,
 * not merely hidden in the UI.
 */
export function PublicInstanceList() {
  return (
    <div className="w-full max-w-md">
      <Card>
        <h1 className="text-xl font-semibold text-ink-strong">
          Submit a ticket
        </h1>
        <p className="mt-2 mb-4 text-sm text-ink-muted">
          Choose an organisation to contact.
        </p>
        <RelayEnvironmentProvider environment={unauthenticatedEnvironment}>
          <RelayErrorBoundary>
            <Suspense fallback={<LoadingIndicator />}>
              <Content />
            </Suspense>
          </RelayErrorBoundary>
        </RelayEnvironmentProvider>
      </Card>
    </div>
  );
}
