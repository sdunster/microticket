import { Suspense } from "react";
import { graphql } from "react-relay";
import type { SettingsQuery } from "./__generated__/SettingsQuery.graphql";
import { useRetryableLazyLoadQuery } from "../components/useRetryableLazyLoadQuery";
import RelayErrorBoundary from "../components/RelayErrorBoundary";
import LoadingIndicator from "../components/LoadingIndicator";
import { PasskeyList } from "./PasskeyList";

const settingsQuery = graphql`
  query SettingsQuery @throwOnFieldError {
    me {
      ...PasskeyList_user
    }
  }
`;

function SettingsContent() {
  const data = useRetryableLazyLoadQuery<SettingsQuery>(settingsQuery, {});
  return (
    <div className="max-w-2xl">
      <h1 className="text-xl font-semibold text-ink-strong">Settings</h1>
      <section className="mt-6">
        <h2 className="text-sm font-semibold tracking-wide text-ink-muted uppercase">
          Passkeys
        </h2>
        <div className="mt-3">
          <PasskeyList user={data.me} />
        </div>
      </section>
    </div>
  );
}

/**
 * `/app/settings` — passkey management. Runs its own query (spreading
 * `PasskeyList`'s own colocated fragment) rather than reusing the shell's
 * `CurrentUserProvider` query, so navigating between ticket views never
 * fetches the passkey list, and visiting Settings never refetches
 * memberships.
 */
export default function Settings() {
  return (
    <RelayErrorBoundary canRetry>
      <Suspense fallback={<LoadingIndicator />}>
        <SettingsContent />
      </Suspense>
    </RelayErrorBoundary>
  );
}
