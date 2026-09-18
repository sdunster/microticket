import { afterEach, describe, expect, it, vi } from "vitest";
import type { RequestParameters } from "relay-runtime";
import { fetchGraphQL } from "./graphql";
import { MutationFieldError } from "./relayErrors";

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function request(
  overrides: Partial<RequestParameters> = {},
): RequestParameters {
  return {
    id: null,
    text: "query Whatever { me { id } }",
    name: "Whatever",
    operationKind: "query",
    metadata: {},
    ...overrides,
  } as RequestParameters;
}

describe("fetchGraphQL", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("sends the Authorization header from a static auth value", async () => {
    const fetchSpy = vi
      .fn()
      .mockResolvedValue(jsonResponse({ data: { me: { id: "1" } } }));
    vi.stubGlobal("fetch", fetchSpy);

    await fetchGraphQL("Bearer abc123", request(), {}, () => {});

    const [, init] = fetchSpy.mock.calls[0] as [string, RequestInit];
    const headers = init.headers as Record<string, string>;
    expect(headers.Authorization).toBe("Bearer abc123");
  });

  it("calls the async auth provider and sends its result", async () => {
    const fetchSpy = vi
      .fn()
      .mockResolvedValue(jsonResponse({ data: { me: { id: "1" } } }));
    vi.stubGlobal("fetch", fetchSpy);

    await fetchGraphQL(
      async () => "Bearer from-provider",
      request(),
      {},
      () => {},
    );

    const [, init] = fetchSpy.mock.calls[0] as [string, RequestInit];
    const headers = init.headers as Record<string, string>;
    expect(headers.Authorization).toBe("Bearer from-provider");
  });

  it("returns partial data for a query with field errors, without throwing", async () => {
    const fetchSpy = vi.fn().mockResolvedValue(
      jsonResponse({
        data: { me: { id: "1", passkeys: null } },
        errors: [{ message: "boom", path: ["me", "passkeys"] }],
      }),
    );
    vi.stubGlobal("fetch", fetchSpy);

    const result = await fetchGraphQL(null, request(), {}, () => {});
    expect(result.data.me.id).toBe("1");
    expect(result.errors).toHaveLength(1);
  });

  it("throws MutationFieldError for a mutation with field errors", async () => {
    const fetchSpy = vi.fn().mockResolvedValue(
      jsonResponse({
        data: { renamePasskey: { id: "1", name: null } },
        errors: [{ message: "boom", path: ["renamePasskey", "name"] }],
      }),
    );
    vi.stubGlobal("fetch", fetchSpy);

    await expect(
      fetchGraphQL(null, request({ operationKind: "mutation" }), {}, () => {}),
    ).rejects.toBeInstanceOf(MutationFieldError);
  });

  it("calls onUnauthorized and throws on a 401", async () => {
    const fetchSpy = vi.fn().mockResolvedValue(jsonResponse({}, 401));
    vi.stubGlobal("fetch", fetchSpy);
    const onUnauthorized = vi.fn();

    await expect(
      fetchGraphQL(null, request(), {}, onUnauthorized),
    ).rejects.toThrow();
    expect(onUnauthorized).toHaveBeenCalledOnce();
  });

  it("throws a descriptive error on a non-401 non-ok response", async () => {
    const fetchSpy = vi.fn().mockResolvedValue(jsonResponse({}, 500));
    vi.stubGlobal("fetch", fetchSpy);

    await expect(fetchGraphQL(null, request(), {}, () => {})).rejects.toThrow(
      /500/,
    );
  });
});
