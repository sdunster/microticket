import { useMemo, useState } from "react";
import { graphql, RelayEnvironmentProvider, useMutation } from "react-relay";
import type { SubmitTicketFormMutation } from "./__generated__/SubmitTicketFormMutation.graphql";
import { createRequesterGraphQLEnvironment } from "../lib/environments";
import { Button } from "../components/ui/Button";
import { FormField } from "../components/ui/FormField";
import TextInput from "../components/ui/TextInput";

const submitTicketMutation = graphql`
  mutation SubmitTicketFormMutation($subject: String!, $body: String!) {
    submitTicket(subject: $subject, body: $body) {
      id
      number
    }
  }
`;

function TicketFields({
  onSubmitted,
}: {
  onSubmitted: (ticketNumber: number) => void;
}) {
  const [subject, setSubject] = useState("");
  const [body, setBody] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [commit, isInFlight] =
    useMutation<SubmitTicketFormMutation>(submitTicketMutation);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    commit({
      variables: { subject, body },
      onCompleted: (response) => onSubmitted(response.submitTicket.number),
      onError: () =>
        setError("Failed to submit your ticket. Please try again."),
    });
  }

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-4">
      <FormField label="Subject" htmlFor="submit-subject">
        <TextInput
          id="submit-subject"
          value={subject}
          onChange={(e) => setSubject(e.target.value)}
          required
          autoFocus
        />
      </FormField>
      <FormField label="How can we help?" htmlFor="submit-body">
        <textarea
          id="submit-body"
          className="min-h-32 w-full rounded-md border border-line bg-surface px-3 py-2 text-sm text-ink focus:border-accent focus:ring-2 focus:ring-accent/25 focus:outline-none"
          value={body}
          onChange={(e) => setBody(e.target.value)}
          required
        />
      </FormField>
      {error && (
        <p role="alert" className="text-sm text-red-600 dark:text-red-400">
          {error}
        </p>
      )}
      <Button type="submit" disabled={isInFlight || !subject || !body}>
        {isInFlight ? "Submitting…" : "Submit ticket"}
      </Button>
    </form>
  );
}

/**
 * The subject/body step, once `verifySubmitCode` has handed us a requester
 * capability token. Its own store-backed Relay environment (`lib/environments.ts`)
 * — scoped to exactly this `(email, instance)` pair, good for `submitTicket`
 * alone, and never persisted (no session is created for a public
 * submission, per the build plan).
 */
export function SubmitTicketForm({
  token,
  onSubmitted,
}: {
  token: string;
  onSubmitted: (ticketNumber: number) => void;
}) {
  const environment = useMemo(
    () => createRequesterGraphQLEnvironment(token),
    [token],
  );
  return (
    <RelayEnvironmentProvider environment={environment}>
      <TicketFields onSubmitted={onSubmitted} />
    </RelayEnvironmentProvider>
  );
}
