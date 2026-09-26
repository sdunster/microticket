import { useState } from "react";
import { graphql, useFragment, useMutation } from "react-relay";
import type { InvoicePaidControl_invoice$key } from "./__generated__/InvoicePaidControl_invoice.graphql";
import type { InvoicePaidControlMutation } from "./__generated__/InvoicePaidControlMutation.graphql";
import { Button } from "../../components/ui/Button";
import { FormField } from "../../components/ui/FormField";
import TextInput from "../../components/ui/TextInput";
import { relayMutationErrorMessage } from "../../lib/relayMutationError";
import { formatDate, localToday } from "../../lib/dates";
import { InvoiceDownloadButton } from "./InvoiceDownloadButton";

const invoicePaidControlFragment = graphql`
  fragment InvoicePaidControl_invoice on Invoice {
    id
    paidDate
  }
`;

/**
 * A finalized invoice's paid status: any member may set or clear it (it's
 * not printed, and isn't part of what finalization froze). Also renders the
 * "Download PDF" button (`InvoiceDownloadButton`) alongside it.
 */
export function InvoicePaidControl({
  invoice,
}: {
  invoice: InvoicePaidControl_invoice$key;
}) {
  const data = useFragment(invoicePaidControlFragment, invoice);
  const [paidDate, setPaidDate] = useState(() => localToday());
  const [error, setError] = useState<string | null>(null);
  const [commit, isSaving] = useMutation<InvoicePaidControlMutation>(graphql`
    mutation InvoicePaidControlMutation($invoiceId: ID!, $paidDate: String) {
      setInvoicePaid(invoiceId: $invoiceId, paidDate: $paidDate) {
        id
        paidDate
      }
    }
  `);

  function commitPaid(date: string | null) {
    setError(null);
    commit({
      variables: { invoiceId: data.id, paidDate: date },
      onError: (err) =>
        setError(
          relayMutationErrorMessage(
            err,
            date ? "Failed to mark as paid." : "Failed to mark as unpaid.",
          ),
        ),
    });
  }

  return (
    <div className="flex flex-col gap-3 rounded-lg border border-line p-4">
      {data.paidDate ? (
        <div className="flex flex-wrap items-center justify-between gap-3">
          <p className="text-sm text-ink">
            Paid on {formatDate(data.paidDate)}
          </p>
          <Button
            variant="secondary"
            disabled={isSaving}
            onClick={() => commitPaid(null)}
          >
            {isSaving ? "Saving…" : "Mark as unpaid"}
          </Button>
        </div>
      ) : (
        <div className="flex flex-wrap items-end gap-3">
          <div className="w-48">
            <FormField label="Paid date" htmlFor="invoice-paid-date">
              <TextInput
                id="invoice-paid-date"
                type="date"
                value={paidDate}
                onChange={(e) => setPaidDate(e.target.value)}
              />
            </FormField>
          </div>
          <Button disabled={isSaving} onClick={() => commitPaid(paidDate)}>
            {isSaving ? "Saving…" : "Mark as paid"}
          </Button>
        </div>
      )}
      {error && (
        <p role="alert" className="text-sm text-red-600 dark:text-red-400">
          {error}
        </p>
      )}
      <InvoiceDownloadButton invoiceId={data.id} />
    </div>
  );
}
