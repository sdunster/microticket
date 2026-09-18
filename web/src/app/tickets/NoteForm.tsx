import { useState } from "react";
import { graphql, useMutation } from "react-relay";
import type { RecordSourceSelectorProxy } from "relay-runtime";
import type { NoteFormMutation } from "./__generated__/NoteFormMutation.graphql";
import { Button } from "../../components/ui/Button";

/**
 * A separate box from `ReplyForm`, not a toggle on it — per the build plan,
 * the distinction between "goes to the customer" and "internal only" is the
 * whole point, so it must not be something a click can accidentally miss.
 * `addInternalNote` never sends mail (enforced server-side); this box's
 * amber styling matches how the note then renders in the thread
 * (`ticketStyles.messageKindContainer.NOTE`).
 */
export function NoteForm({ ticketId }: { ticketId: string }) {
  const [body, setBody] = useState("");
  const [error, setError] = useState<string | null>(null);

  const [commit, isSaving] = useMutation<NoteFormMutation>(graphql`
    mutation NoteFormMutation($ticketId: ID!, $body: String!) {
      addInternalNote(ticketId: $ticketId, body: $body) {
        id
        ...MessageItem_message
      }
    }
  `);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const text = body.trim();
    if (!text || isSaving) return;
    setError(null);

    const updater = (store: RecordSourceSelectorProxy) => {
      const ticketRecord = store.get(ticketId);
      const newMessage = store.getRootField("addInternalNote");
      if (!ticketRecord || !newMessage) return;
      const existing = ticketRecord.getLinkedRecords("messages") ?? [];
      ticketRecord.setLinkedRecords([...existing, newMessage], "messages");
    };

    commit({
      variables: { ticketId, body: text },
      updater,
      onCompleted: () => setBody(""),
      onError: () =>
        setError("Failed to save note. Your draft is unchanged — try again."),
    });
  }

  return (
    <form
      onSubmit={handleSubmit}
      className="flex flex-col gap-3 rounded-lg border border-amber-300 bg-amber-50 p-4 dark:border-amber-800 dark:bg-amber-950/40"
    >
      <h3 className="text-sm font-semibold text-amber-800 dark:text-amber-300">
        Internal note — never emailed
      </h3>
      <textarea
        aria-label="Internal note"
        className="min-h-20 w-full rounded-md border border-amber-300 bg-surface px-3 py-2 text-sm text-ink focus:border-accent focus:ring-2 focus:ring-accent/25 focus:outline-none dark:border-amber-800"
        placeholder="Leave a note for your team. The requester will never see this."
        value={body}
        onChange={(e) => setBody(e.target.value)}
      />
      {error && (
        <p role="alert" className="text-sm text-red-600 dark:text-red-400">
          {error}
        </p>
      )}
      <Button
        type="submit"
        variant="secondary"
        disabled={!body.trim() || isSaving}
        className="self-start"
      >
        {isSaving ? "Saving…" : "Add note"}
      </Button>
    </form>
  );
}
