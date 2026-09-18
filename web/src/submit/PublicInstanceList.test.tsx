import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { setupServer } from "msw/node";
import { graphql, HttpResponse } from "msw";
import { MemoryRouter } from "react-router";
import { getGraphQLEndpoint } from "../lib/api";
import { PublicInstanceList } from "./PublicInstanceList";

const relayEndpoint = graphql.link(getGraphQLEndpoint());
const server = setupServer();

beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe("PublicInstanceList", () => {
  it("lists only what the unauthenticated publicInstances query returns — never a non-public instance", async () => {
    // The API itself filters to `public_submission_enabled` instances (see
    // `publicInstances`'s doc comment in schema.graphql) — this response
    // stands in for that filtering already having happened server-side.
    // Ridgeline (public submission off) is deliberately absent, not merely
    // included-then-hidden, which is what this test guards against.
    server.use(
      relayEndpoint.query("PublicInstanceListQuery", () =>
        HttpResponse.json({
          data: {
            publicInstances: [{ name: "Acme Support", slug: "acme" }],
          },
        }),
      ),
    );

    render(
      <MemoryRouter>
        <PublicInstanceList />
      </MemoryRouter>,
    );

    expect(await screen.findByText("Acme Support")).toBeInTheDocument();
    expect(screen.queryByText("Ridgeline Help Desk")).not.toBeInTheDocument();
    expect(screen.getAllByRole("link")).toHaveLength(1);
  });

  it("shows an empty state when no instance accepts public submissions", async () => {
    server.use(
      relayEndpoint.query("PublicInstanceListQuery", () =>
        HttpResponse.json({ data: { publicInstances: [] } }),
      ),
    );

    render(
      <MemoryRouter>
        <PublicInstanceList />
      </MemoryRouter>,
    );

    expect(
      await screen.findByText(
        "No organisations currently accept public ticket submissions here.",
      ),
    ).toBeInTheDocument();
  });
});
