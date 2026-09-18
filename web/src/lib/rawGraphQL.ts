import { getGraphQLEndpoint } from "./api";

/**
 * Standard headers for every request to the API, Relay or raw.
 * `X-Client-Version` lets the API log/attribute which build a request came
 * from; it's non-secret and safe to send unauthenticated.
 */
export function clientHeaders(): Record<string, string> {
  return {
    "X-Client-Version": import.meta.env.VITE_CLIENT_VERSION ?? "dev",
  };
}

/**
 * A minimal, un-authenticated-by-default GraphQL POST, for the handful of
 * call sites that must run *before* a Relay environment exists: the login
 * page's email-code mutations and the passkey login ceremony. Everything
 * else goes through `fetchGraphQL` (see `lib/graphql.ts`), which is the one
 * seam Relay's `Network` layer calls — this helper is deliberately not that,
 * and nothing outside `auth/` should reach for it.
 */
export async function rawGraphQL<TData = Record<string, unknown>>(
  query: string,
  variables: Record<string, unknown> = {},
  authHeader?: string,
): Promise<{ data?: TData; errors?: ReadonlyArray<{ message?: string }> }> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...clientHeaders(),
  };
  if (authHeader) {
    headers["Authorization"] = authHeader;
  }
  const resp = await fetch(getGraphQLEndpoint(), {
    method: "POST",
    headers,
    body: JSON.stringify({ query, variables }),
    cache: "no-store",
  });
  if (!resp.ok) {
    throw new Error(`HTTP ${resp.status}`);
  }
  return (await resp.json()) as {
    data?: TData;
    errors?: ReadonlyArray<{ message?: string }>;
  };
}
