import { type ReactNode } from "react";
import { graphql } from "react-relay";
import type { CurrentUserProviderQuery } from "./__generated__/CurrentUserProviderQuery.graphql";
import { CurrentUserContext } from "./CurrentUserContext";
import { useRetryableLazyLoadQuery } from "../components/useRetryableLazyLoadQuery";

// `me`'s scalar fields are exposed app-wide through `CurrentUserContext` /
// `useCurrentUser()` rather than read in this file directly — that
// non-Relay channel is invisible to the relay/unused-fields lint rule, so
// disable it here rather than threading a "current user badge" component's
// fragment through just to satisfy it. Likewise `...InstanceSwitcher_user`:
// InstanceSwitcher does declare and read that fragment itself (via the
// context value this provider hands out), but relay/must-colocate-fragment-spreads
// only sees a same-file JS reference, not a cross-file one threaded through
// context — which is the whole point of "a page runs one query that
// spreads [components'] fragments" (see the build plan's "Relay, done
// differently"). This is the one query that runs for every `/app/*` page —
// anything more page-specific (e.g. the passkey list on Settings) gets its
// own query at the point it's needed, not appended here.
//
// `memberships { instance { id } }` is selected plainly (not via a
// colocated fragment) for the same reason as the scalars above: `AppRoute`'s
// index redirect needs to know synchronously whether a superuser has *any*
// membership, and that data is already fetched by `InstanceSwitcher_user`
// below — duplicating the selection (GraphQL/Relay dedupe it) beats adding a
// second query or threading a callback out of InstanceSwitcher just to
// unmask one array's length.
/* eslint-disable relay/unused-fields, relay/must-colocate-fragment-spreads */
const currentUserProviderQuery = graphql`
  query CurrentUserProviderQuery @throwOnFieldError {
    me {
      id
      email
      name
      enabled
      isSuperuser
      memberships {
        instance {
          id
        }
      }
      ...InstanceSwitcher_user
    }
  }
`;
/* eslint-enable relay/unused-fields, relay/must-colocate-fragment-spreads */

/**
 * Fetches the authenticated user's own record and makes it available via
 * `useCurrentUser()`. Must be rendered inside a `RelayEnvironmentProvider`
 * and a Suspense boundary (see `AuthenticatedSession`).
 */
export function CurrentUserProvider({ children }: { children: ReactNode }) {
  const data = useRetryableLazyLoadQuery<CurrentUserProviderQuery>(
    currentUserProviderQuery,
    {},
  );

  return (
    <CurrentUserContext.Provider value={data.me}>
      {children}
    </CurrentUserContext.Provider>
  );
}
