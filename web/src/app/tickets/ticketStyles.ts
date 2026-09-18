import { tw } from "../../lib/tw";

/** `TicketStatusType` / `TicketStatusFilterType` badge colors. */
export const statusBadge: Record<string, string> = {
  OPEN: tw`bg-green-100 text-green-800 dark:bg-green-900/40 dark:text-green-300`,
  CLOSED: tw`bg-surface-sunken text-ink-muted`,
  DELETED: tw`bg-red-100 text-red-800 dark:bg-red-900/40 dark:text-red-300`,
};

export const statusBadgeBase = tw`inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium capitalize`;

/**
 * `TicketMessageKindType` message-bubble styling. `note` is deliberately the
 * most visually distinct — amber, with its own label — so an agent can never
 * mistake it for something the requester saw. `system` is intentionally
 * *not* a bubble at all (see `MessageItem`) — it reads as a log line, not a
 * reply from anyone.
 */
export const messageKindContainer: Record<string, string> = {
  INBOUND: tw`border border-line bg-surface-raised`,
  REPLY: tw`border border-accent/30 bg-accent/10`,
  NOTE: tw`border border-amber-300 bg-amber-50 dark:border-amber-800 dark:bg-amber-950/40`,
};

export const messageKindLabel: Record<string, string> = {
  INBOUND: tw`text-ink-muted`,
  REPLY: tw`text-accent-dark dark:text-accent-light`,
  NOTE: tw`text-amber-800 dark:text-amber-300`,
};
