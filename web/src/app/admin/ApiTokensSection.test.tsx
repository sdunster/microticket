import { Suspense } from "react";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { setupServer } from "msw/node";
import { graphql as mswGraphql, HttpResponse } from "msw";
import {
  graphql,
  useLazyLoadQuery,
  RelayEnvironmentProvider,
} from "react-relay";
import { MemoryRouter } from "react-router";
import { getGraphQLEndpoint } from "../../lib/api";
import { createUnauthenticatedGraphQLEnvironment } from "../../lib/environments";
import { ApiTokensSection } from "./ApiTokensSection";
import type { ApiTokensSectionHarnessQuery } from "./__generated__/ApiTokensSectionHarnessQuery.graphql";

const relayEndpoint = mswGraphql.link(getGraphQLEndpoint());
const server = setupServer();

beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

const harnessQuery = graphql`
  query ApiTokensSectionHarnessQuery($id: ID!) @throwOnFieldError {
    adminInstance(id: $id) {
      ...ApiTokensSection_instance
    }
  }
`;

function Harness({ id }: { id: string }) {
  const data = useLazyLoadQuery<ApiTokensSectionHarnessQuery>(harnessQuery, {
    id,
  });
  if (!data.adminInstance) return null;
  return <ApiTokensSection instance={data.adminInstance} />;
}

function renderHarness() {
  const environment = createUnauthenticatedGraphQLEnvironment();
  return render(
    <MemoryRouter>
      <RelayEnvironmentProvider environment={environment}>
        <Suspense fallback="loading">
          <Harness id="inst-1" />
        </Suspense>
      </RelayEnvironmentProvider>
    </MemoryRouter>,
  );
}

describe("ApiTokensSection — create flow", () => {
  it("shows the freshly minted secret exactly once, then lists the token", async () => {
    server.use(
      relayEndpoint.query("ApiTokensSectionHarnessQuery", () =>
        HttpResponse.json({
          data: {
            adminInstance: {
              __typename: "Instance",
              id: "inst-1",
              apiTokens: [],
            },
          },
        }),
      ),
      relayEndpoint.mutation("ApiTokensSectionCreateMutation", () =>
        HttpResponse.json({
          data: {
            createApiToken: {
              token: "mta_abc123.supersecretvalue",
              apiToken: {
                __typename: "ApiTokenInfo",
                id: "tok-1",
                name: "Partner portal",
                enabled: true,
                createdAt: 1_700_000_000,
                lastUsedAt: null,
                createdBy: {
                  __typename: "User",
                  id: "u1",
                  name: "Alice Owner",
                },
              },
            },
          },
        }),
      ),
    );

    renderHarness();

    await screen.findByText("No API tokens yet.");

    fireEvent.change(screen.getByLabelText("New token name"), {
      target: { value: "Partner portal" },
    });
    fireEvent.click(screen.getByText("Create token"));

    expect(await screen.findByText(/Copy this token now/)).toBeInTheDocument();
    expect(screen.getByLabelText("New API token secret")).toHaveValue(
      "mta_abc123.supersecretvalue",
    );

    // The new row shows up in the list, appended via the updater.
    await waitFor(() =>
      expect(
        screen.getByLabelText("Name for token Partner portal"),
      ).toHaveValue("Partner portal"),
    );

    // Dismissing hides the secret for good — nothing re-reveals it.
    fireEvent.click(screen.getByText("Dismiss"));
    expect(screen.queryByText(/Copy this token now/)).not.toBeInTheDocument();
  });
});
