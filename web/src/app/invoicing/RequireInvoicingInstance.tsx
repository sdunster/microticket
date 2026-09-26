import type { ReactNode } from "react";
import { useSelectedInstance } from "../SelectedInstanceContext";
import type { SelectedInstance } from "../selectedInstance";

/**
 * Renders `children` only when the switcher's selection is an invoicing
 * instance — otherwise the same dashed "pick an instance" panel
 * `TicketListPage` shows with nothing selected. Every invoicing query
 * (`projects`, `invoicingSettings`, …) is meaningless — or, for `projects`,
 * an outright validation error — against a support instance, so a deep link
 * reached with a support instance selected lands here instead of on an
 * error page.
 */
export function RequireInvoicingInstance({
  children,
  purpose,
}: {
  /** Completes "Pick an invoicing instance to …". */
  purpose: string;
  children: (instance: SelectedInstance) => ReactNode;
}) {
  const instance = useSelectedInstance();

  if (!instance || instance.kind !== "INVOICING") {
    return (
      <div className="rounded-lg border border-dashed border-line p-10 text-center text-ink-muted">
        <p>Pick an invoicing instance to {purpose}.</p>
        {instance && (
          <p className="mt-1 text-sm">
            {instance.name} is a support instance — switch instances in the menu
            above.
          </p>
        )}
      </div>
    );
  }

  return children(instance);
}
