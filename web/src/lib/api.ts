/**
 * Resolves the GraphQL endpoint. In dev mode we always talk to the local API
 * server; everywhere else (a production build) `VITE_API_URL` is required —
 * failing loudly at call time beats silently POSTing to `localhost` in prod.
 */
export function getGraphQLEndpoint(): string {
  if (import.meta.env.MODE === "development") {
    return "http://localhost:8000/";
  }
  if (import.meta.env.PROD && !import.meta.env.VITE_API_URL) {
    throw new Error("VITE_API_URL must be set for a production build");
  }
  return import.meta.env.VITE_API_URL ?? "http://localhost:8000/";
}
