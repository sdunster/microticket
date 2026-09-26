import { useEffect, useRef, useTransition } from "react";
import { graphql, usePaginationFragment } from "react-relay";
import type { BillableItemList_query$key } from "./__generated__/BillableItemList_query.graphql";
import type {
  BillableItemFilterType,
  BillableItemListPaginationQuery,
} from "./__generated__/BillableItemListPaginationQuery.graphql";
import { Button } from "../../components/ui/Button";
import { BillableItemHeader, BillableItemRow } from "./BillableItemRow";

export const BILLABLE_ITEM_PAGE_SIZE = 25;

const billableItemListFragment = graphql`
  fragment BillableItemList_query on QueryRoot
  @refetchable(queryName: "BillableItemListPaginationQuery")
  @argumentDefinitions(
    instanceId: { type: "ID!" }
    projectId: { type: "ID" }
    filter: { type: "BillableItemFilterType!" }
    count: { type: "Int", defaultValue: 25 }
    cursor: { type: "String" }
  ) {
    billableItems(
      instanceId: $instanceId
      projectId: $projectId
      filter: $filter
      first: $count
      after: $cursor
    ) @connection(key: "BillableItemList_billableItems") {
      __id
      edges {
        node {
          id
          ...BillableItemRow_item
        }
      }
    }
  }
`;

/**
 * A paginated list of billable items: the instance-wide page (with a
 * project column) or one project's section (with Edit/Delete instead).
 * Newest date first, "Load more" at the bottom.
 *
 * Like `TicketList`, this owns refetching: the parent's query runs once per
 * mount, and a new `filter` — or a bumped `refreshKey`, after the parent
 * adds an item — refetches through this fragment's own `refetch` inside a
 * transition, so the current rows stay (dimmed) instead of flashing a
 * Suspense fallback. A refetch deliberately goes back to the first page:
 * that's where a new or re-dated item lands in date order.
 */
export function BillableItemList({
  query,
  filter,
  showProject,
  editable,
  currency,
  emptyMessage,
  refreshKey = 0,
}: {
  query: BillableItemList_query$key;
  filter: BillableItemFilterType;
  showProject: boolean;
  editable: boolean;
  currency: string;
  emptyMessage: string;
  refreshKey?: number;
}) {
  const { data, loadNext, hasNext, isLoadingNext, refetch } =
    usePaginationFragment<
      BillableItemListPaginationQuery,
      BillableItemList_query$key
    >(billableItemListFragment, query);
  const [isRefetching, startTransition] = useTransition();

  function refresh(nextFilter: BillableItemFilterType) {
    startTransition(() => {
      refetch({ filter: nextFilter }, { fetchPolicy: "network-only" });
    });
  }

  const mounted = useRef({ filter, refreshKey });
  useEffect(() => {
    if (
      mounted.current.filter === filter &&
      mounted.current.refreshKey === refreshKey
    ) {
      return;
    }
    mounted.current = { filter, refreshKey };
    startTransition(() => {
      refetch({ filter }, { fetchPolicy: "network-only" });
    });
  }, [filter, refreshKey, refetch]);

  const connection = data.billableItems;
  const edges = connection.edges;

  if (edges.length === 0) {
    return (
      <div
        className={`rounded-lg border border-dashed border-line p-10 text-center text-ink-muted ${isRefetching ? "opacity-60" : ""}`}
      >
        {emptyMessage}
      </div>
    );
  }

  return (
    <div
      className={`flex flex-col gap-4 ${isRefetching ? "opacity-60" : ""}`}
      aria-busy={isRefetching}
    >
      <div className="rounded-lg border border-line">
        <BillableItemHeader
          showProject={showProject}
          editable={editable}
          currency={currency}
        />
        <ul className="flex flex-col divide-y divide-line-faint">
          {edges.map((edge) => (
            <li key={edge.node.id}>
              <BillableItemRow
                item={edge.node}
                showProject={showProject}
                editable={editable}
                currency={currency}
                connectionId={connection.__id}
                onChanged={() => refresh(filter)}
              />
            </li>
          ))}
        </ul>
      </div>

      {hasNext && (
        <Button
          variant="secondary"
          className="self-center"
          disabled={isLoadingNext}
          onClick={() => loadNext(BILLABLE_ITEM_PAGE_SIZE)}
        >
          {isLoadingNext ? "Loading…" : "Load more"}
        </Button>
      )}
    </div>
  );
}
