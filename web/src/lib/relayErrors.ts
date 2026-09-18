/**
 * Thrown by `fetchGraphQL` when a **mutation** response comes back with a
 * populated `errors` array. Unlike a query (which returns its partial `data`
 * straight to Relay, since a nested field failure there just means one field
 * is missing), a mutation that reports a field error already performed its
 * write — but something in the response couldn't be read back, so treating it
 * as a normal thrown network/GraphQL error is the safer default: it reaches
 * `onError`, not `onCompleted`.
 */
export class MutationFieldError extends Error {
  readonly errors: ReadonlyArray<{ message?: string; path?: unknown }>;

  constructor(errors: ReadonlyArray<{ message?: string; path?: unknown }>) {
    const first = errors[0];
    super(first?.message ?? "GraphQL mutation returned errors");
    this.name = "MutationFieldError";
    this.errors = errors;
  }
}
