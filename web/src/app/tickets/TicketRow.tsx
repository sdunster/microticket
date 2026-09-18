import { graphql, useFragment } from "react-relay";
import { Link } from "react-router";
import type { TicketRow_ticket$key } from "./__generated__/TicketRow_ticket.graphql";
import { formatRelativeTime } from "../../lib/relativeTime";
import { statusBadge, statusBadgeBase } from "./ticketStyles";

const ticketRowFragment = graphql`
  fragment TicketRow_ticket on Ticket {
    id
    number
    subject
    status
    requesterEmails
    lastActivityAt
    assignee {
      id
      name
    }
    messages {
      id
      attachments {
        filename
      }
    }
  }
`;

function AttachmentPaperclip() {
  return (
    <svg
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      className="size-4 shrink-0 text-ink-muted"
      aria-hidden="true"
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M14.5 6.5 8.4 12.6a2 2 0 1 0 2.83 2.83l6.1-6.1a4 4 0 1 0-5.66-5.66l-6.1 6.1a6 6 0 1 0 8.49 8.49"
      />
    </svg>
  );
}

/**
 * One row of a ticket list: what an agent triages on — number, subject,
 * requester, assignee, status, last activity, and an attachment indicator.
 *
 * `messages { attachments { filename } }` is the only way the schema
 * exposes "does this ticket have an attachment" — `Ticket` has no
 * lightweight `hasAttachments` field, so this fetches every message's
 * attachment list (not bodies) for every row on the page. Acceptable for a
 * helpdesk queue's page sizes, but it is one extra DynamoDB query per row;
 * see the step 8b report for the tradeoff.
 */
export function TicketRow({ ticket }: { ticket: TicketRow_ticket$key }) {
  const data = useFragment(ticketRowFragment, ticket);
  const attachmentFilenames = data.messages.flatMap((m) =>
    m.attachments.map((a) => a.filename),
  );
  const [primaryRequester, ...moreRequesters] = data.requesterEmails;

  return (
    <Link
      to={`/app/tickets/${data.id}`}
      className="grid grid-cols-[auto_1fr_auto] items-center gap-x-4 gap-y-1 rounded-md p-3 no-underline transition-colors hover:bg-surface-raised sm:grid-cols-[3rem_1fr_auto_auto_auto]"
    >
      <span className="text-sm font-medium text-ink-muted tabular-nums">
        #{data.number}
      </span>

      <span className="col-span-2 flex min-w-0 items-center gap-2 sm:col-span-1">
        <span className="truncate font-medium text-ink-strong">
          {data.subject}
        </span>
        {attachmentFilenames.length > 0 && (
          <span title={`Attachments: ${attachmentFilenames.join(", ")}`}>
            <AttachmentPaperclip />
          </span>
        )}
      </span>

      <span className="truncate text-sm text-ink-muted">
        {primaryRequester}
        {moreRequesters.length > 0 && ` +${moreRequesters.length}`}
      </span>

      <span className="text-sm text-ink-muted">
        {data.assignee ? data.assignee.name : "Unassigned"}
      </span>

      <span className={`${statusBadgeBase} ${statusBadge[data.status] ?? ""}`}>
        {data.status.toLowerCase()}
      </span>

      <span
        className="col-span-3 text-xs text-ink-muted sm:col-span-1"
        title={new Date(data.lastActivityAt * 1000).toLocaleString()}
      >
        {formatRelativeTime(data.lastActivityAt)}
      </span>
    </Link>
  );
}
