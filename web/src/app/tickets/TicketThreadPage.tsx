import { Suspense } from "react";
import { graphql } from "react-relay";
import { useParams } from "react-router";
import type { TicketThreadPageQuery } from "./__generated__/TicketThreadPageQuery.graphql";
import { useSelectedInstance } from "../SelectedInstanceContext";
import { useRetryableLazyLoadQuery } from "../../components/useRetryableLazyLoadQuery";
import RelayErrorBoundary from "../../components/RelayErrorBoundary";
import LoadingIndicator from "../../components/LoadingIndicator";
import { ButtonLink } from "../../components/ui/Button";
import { TicketThread } from "./TicketThread";

const ticketThreadPageQuery = graphql`
  query TicketThreadPageQuery($id: ID!, $isOwner: Boolean!) @throwOnFieldError {
    ticket(id: $id) {
      id
      ...TicketThread_ticket @arguments(isOwner: $isOwner)
    }
  }
`;

function Content({ id, isOwner }: { id: string; isOwner: boolean }) {
  const data = useRetryableLazyLoadQuery<TicketThreadPageQuery>(
    ticketThreadPageQuery,
    { id, isOwner },
  );

  if (!data.ticket) {
    return (
      <div className="rounded-lg border border-dashed border-line p-10 text-center text-ink-muted">
        <p className="font-medium text-ink">Ticket not found</p>
        <p className="mt-1 text-sm">
          It may not exist, or you may not have access to it.
        </p>
        <ButtonLink to="/app/tickets/open" variant="secondary" className="mt-4">
          Back to tickets
        </ButtonLink>
      </div>
    );
  }

  return <TicketThread ticket={data.ticket} />;
}

/**
 * `/app/tickets/:id` — the thread view. `isOwner` gates
 * `AssigneeControl`'s full member picker (see its doc comment); it's read
 * from the currently-selected instance rather than a fresh per-ticket
 * lookup, on the assumption that a ticket link is only ever reached from
 * within that instance's own queue — the API's own authorization is the
 * real boundary regardless (a non-owner requesting `members` would get
 * `FORBIDDEN`, not a workaround).
 */
export function TicketThreadPage() {
  const { id } = useParams<{ id: string }>();
  const instance = useSelectedInstance();

  if (!id) return null;

  return (
    <RelayErrorBoundary canRetry>
      <Suspense fallback={<LoadingIndicator />}>
        <Content id={id} isOwner={instance?.role === "OWNER"} />
      </Suspense>
    </RelayErrorBoundary>
  );
}
