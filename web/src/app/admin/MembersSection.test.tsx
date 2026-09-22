import { Suspense } from "react";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
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
import { MembersSection } from "./MembersSection";
import type { MembersSectionHarnessQuery } from "./__generated__/MembersSectionHarnessQuery.graphql";

const relayEndpoint = mswGraphql.link(getGraphQLEndpoint());
const server = setupServer();

beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

const harnessQuery = graphql`
  query MembersSectionHarnessQuery($id: ID!) @throwOnFieldError {
    adminInstance(id: $id) {
      ...MembersSection_instance
    }
    ...MembersSection_query
  }
`;

function Harness({ id }: { id: string }) {
  const data = useLazyLoadQuery<MembersSectionHarnessQuery>(harnessQuery, {
    id,
  });
  if (!data.adminInstance) return null;
  return <MembersSection instance={data.adminInstance} query={data} />;
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

describe("MembersSection — add-member picker", () => {
  it("excludes users who are already members", async () => {
    server.use(
      relayEndpoint.query("MembersSectionHarnessQuery", () =>
        HttpResponse.json({
          data: {
            adminInstance: {
              __typename: "Instance",
              id: "inst-1",
              members: [
                {
                  role: "OWNER",
                  user: {
                    __typename: "User",
                    id: "u1",
                    name: "Alice Owner",
                    email: "alice@example.com",
                  },
                },
              ],
            },
            adminUsers: [
              {
                __typename: "User",
                id: "u1",
                name: "Alice Owner",
                email: "alice@example.com",
              },
              {
                __typename: "User",
                id: "u2",
                name: "Bob Agent",
                email: "bob@example.com",
              },
            ],
          },
        }),
      ),
    );

    renderHarness();

    await screen.findByText("Alice Owner");

    const picker = screen.getByLabelText("User to add");
    const options = Array.from(picker.querySelectorAll("option")).map(
      (o) => o.textContent,
    );
    expect(options).toContain("Bob Agent (bob@example.com)");
    expect(options).not.toContain("Alice Owner (alice@example.com)");
  });
});
