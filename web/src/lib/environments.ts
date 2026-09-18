import {
  Environment,
  Network,
  RecordSource,
  Store,
  type FetchFunction,
} from "relay-runtime";
import { fetchGraphQL } from "./graphql";

type GetTokenFn = () => Promise<string>;

/**
 * The environment for `/app/*`: a bearer token (read lazily per-request via
 * `getToken`, so it's never stale) plus a real `Store`, since this is where
 * Relay's cache, connections and mutation updaters actually matter.
 *
 * `onTokenError` fires when `getToken` itself throws (no token to send —
 * shouldn't normally happen, since the authenticated tree only mounts once a
 * token exists). `onUnauthorized` fires on a definitive 401 from the server
 * (expired/revoked token).
 *
 * Step 8b adds a third, requester-token environment for the public submit
 * form (`/submit/:slug`) alongside this one and the unauthenticated
 * singleton below — same shape, a fixed capability token instead of a
 * refreshable bearer one.
 */
export function createAuthenticatedGraphQLEnvironment(
  getToken: GetTokenFn,
  onTokenError: () => void,
  onUnauthorized: () => void,
): Environment {
  const _fetchGraphQL: FetchFunction = async (request, variables) => {
    let token: string;
    try {
      token = await getToken();
    } catch (err) {
      console.error("Failed to get auth token:", err);
      onTokenError();
      throw new Error("Failed to get auth token", { cause: err });
    }

    return await fetchGraphQL(`Bearer ${token}`, request, variables, () => {
      onUnauthorized();
      throw new Error("Unauthorized from server");
    });
  };

  return new Environment({
    network: Network.create(_fetchGraphQL),
    store: new Store(new RecordSource()),
  });
}

/**
 * A store-less singleton for one-off unauthenticated requests (e.g.
 * `publicInstances`). No caching is appropriate here — every consumer wants
 * a fresh network hit, and there is no bearer identity to key a cache by.
 */
export function createUnauthenticatedGraphQLEnvironment(): Environment {
  const _fetchGraphQL: FetchFunction = async (request, variables) => {
    return await fetchGraphQL(null, request, variables, () => {
      console.log("Unauthorized in unauthenticated environment");
    });
  };

  return new Environment({
    network: Network.create(_fetchGraphQL),
  });
}

export const unauthenticatedEnvironment =
  createUnauthenticatedGraphQLEnvironment();
