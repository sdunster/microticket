import { useEffect, useRef } from "react";
import { graphql, usePaginationFragment } from "react-relay";
import type { TicketList_query$key } from "./__generated__/TicketList_query.graphql";
import type { TicketListPaginationQuery } from "./__generated__/TicketListPaginationQuery.graphql";
import type { TicketStatusFilterType } from "./__generated__/TicketListPageQuery.graphql";
import { TicketRow } from "./TicketRow";
import { Button } from "../../components/ui/Button";

const PAGE_SIZE = 25;

const ticketListFragment = graphql`
  fragment TicketList_query on QueryRoot
  @refetchable(queryName: "TicketListPaginationQuery")
  @argumentDefinitions(
    instanceId: { type: "ID!" }
    status: { type: "TicketStatusFilterType!" }
    assignedTo: { type: "ID" }
    count: { type: "Int", defaultValue: 25 }
    cursor: { type: "String" }
  ) {
    tickets(
      instanceId: $instanceId
      status: $status
      assignedTo: $assignedTo
      first: $count
      after: $cursor
    ) @connection(key: "TicketList_tickets") {
      edges {
        node {
          id
          ...TicketRow_ticket
        }
      }
    }
  }
`;

interface TicketListProps {
  query: TicketList_query$key;
  status: TicketStatusFilterType;
  assignedTo: string | null;
  emptyMessage: string;
}

/**
 * The paginated connection body: rows plus "Load more". Also owns
 * refetching when the *filter* (status / assignedTo) changes — the parent
 * page's `useLazyLoadQuery` only ever runs once per selected instance (see
 * `TicketListPage`), so switching tabs must not remount this subtree; it
 * calls this same fragment's own `refetch` (the `@refetchable` machinery
 * `usePaginationFragment` is built on) instead. That single fragment is
 * doing both jobs the build plan calls out separately — "usePaginationFragment
 * + @connection" for Load More and "@refetchable ... for filter changes" —
 * because both are backed by the exact same generated refetch query here;
 * a second, separate `useRefetchableFragment` over the same field would be
 * redundant with what `usePaginationFragment` already exposes.
 */
export function TicketList({
  query,
  status,
  assignedTo,
  emptyMessage,
}: TicketListProps) {
  const { data, loadNext, hasNext, isLoadingNext, refetch } =
    usePaginationFragment<TicketListPaginationQuery, TicketList_query$key>(
      ticketListFragment,
      query,
    );

  const mountedFilter = useRef({ status, assignedTo });
  useEffect(() => {
    if (
      mountedFilter.current.status === status &&
      mountedFilter.current.assignedTo === assignedTo
    ) {
      return;
    }
    mountedFilter.current = { status, assignedTo };
    refetch({ status, assignedTo }, { fetchPolicy: "network-only" });
  }, [status, assignedTo, refetch]);

  const edges = data.tickets.edges;

  if (edges.length === 0) {
    return (
      <div className="rounded-lg border border-dashed border-line p-10 text-center text-ink-muted">
        {emptyMessage}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <ul className="flex flex-col divide-y divide-line-faint rounded-lg border border-line">
        {edges.map((edge) => (
          <li key={edge.node.id}>
            <TicketRow ticket={edge.node} />
          </li>
        ))}
      </ul>

      {hasNext && (
        <Button
          variant="secondary"
          className="self-center"
          disabled={isLoadingNext}
          onClick={() => loadNext(PAGE_SIZE)}
        >
          {isLoadingNext ? "Loading…" : "Load more"}
        </Button>
      )}
    </div>
  );
}

export { PAGE_SIZE };
