import { getGraphQLEndpoint } from "./api";
import { clientHeaders } from "./rawGraphQL";
import { MutationFieldError } from "./relayErrors";
import type { RequestParameters, Variables } from "relay-runtime";

/**
 * Supplies the `Authorization` header value for a request: a static full
 * header value (or `null` for none), or an async producer called with no
 * arguments — used by the authenticated environment, which reads the token
 * from storage lazily per-request rather than closing over a value that
 * could go stale.
 */
export type AuthHeaderProvider = string | null | (() => Promise<string | null>);

/**
 * The single seam every Relay environment's `Network` goes through. Owns:
 *  - request headers (`X-Client-Version`, `Authorization`)
 *  - 401 handling (`onUnauthorized`, then a thrown error so Relay doesn't
 *    treat the response as valid data)
 *  - classifying a response's `errors` array: a **query** with partial
 *    `errors` returns its (possibly incomplete) `data` straight to Relay —
 *    that's normal for a query touching a field the caller can't read. A
 *    **mutation** with `errors` throws `MutationFieldError` instead: the
 *    write already happened, but something in the read-back failed, which
 *    is a failure from the caller's point of view, not a partial success.
 *
 * Nothing else in this app may call `fetch` against the API — everything
 * else goes through a Relay environment built on this function, except the
 * handful of pre-auth call sites in `auth/` that use `rawGraphQL` instead
 * (see that module's doc comment for why).
 */
export async function fetchGraphQL(
  authHeader: AuthHeaderProvider,
  request: RequestParameters,
  variables: Variables,
  onUnauthorized: () => void,
) {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...clientHeaders(),
  };
  const authValue =
    typeof authHeader === "function" ? await authHeader() : authHeader;
  if (authValue) {
    headers["Authorization"] = authValue;
  }

  const endpoint = getGraphQLEndpoint();
  let resp: Response;
  try {
    resp = await fetch(endpoint, {
      method: "POST",
      headers,
      body: JSON.stringify({ query: request.text, variables }),
      cache: "no-store",
    });
  } catch (error) {
    throw new Error(
      `Failed to fetch GraphQL endpoint ${endpoint}: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }

  if (resp.status === 401) {
    onUnauthorized();
    throw new Error("Unauthorized");
  }
  if (!resp.ok) {
    throw new Error(`GraphQL request failed with HTTP ${resp.status}`);
  }

  const responseBody = await resp.json();

  const errors:
    ReadonlyArray<{ message?: string; path?: unknown }> | undefined =
    Array.isArray(responseBody?.errors) ? responseBody.errors : undefined;

  if (errors && errors.length > 0) {
    if (request.operationKind === "mutation") {
      // The write already happened (data may be non-null); something nested
      // in the response failed to resolve. Treat this as a mutation failure
      // rather than letting a silently-null field reach onCompleted.
      throw new MutationFieldError(errors);
    }
    // Queries: hand the partial response to Relay as-is. The field(s) that
    // errored come back `null`/missing in `data`, and whatever rendered
    // that selection is responsible for handling it (e.g. `@throwOnFieldError`
    // on the query, which turns this into a thrown error at the read site).
    console.warn(
      `GraphQL query ${request.name ?? "unknown"} returned ${errors.length} field error(s):`,
      errors,
    );
  }

  return responseBody;
}
