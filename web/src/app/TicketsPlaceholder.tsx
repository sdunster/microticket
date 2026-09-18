import { useSelectedInstance } from "./SelectedInstanceContext";

/**
 * Placeholder for the ticket-list and thread views — step 8b builds these
 * (Relay connections, `@appendEdge`/`@deleteEdge` updaters, the reply box).
 * Kept here only so `/app/tickets/*` routes to something rather than 404ing.
 */
export function TicketsPlaceholder({ title }: { title: string }) {
  const instance = useSelectedInstance();
  return (
    <div className="rounded-lg border border-dashed border-line p-8 text-center text-ink-muted">
      <p className="font-medium text-ink">{title}</p>
      <p className="mt-1 text-sm">
        {instance
          ? `Ticket lists for ${instance.name} land in step 8b.`
          : "Ticket lists land in step 8b."}
      </p>
    </div>
  );
}
