import { Suspense, useEffect, useState } from "react";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import UserEvent from "@testing-library/user-event";
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
import { TicketList } from "./TicketList";
import type { TicketListHarnessQuery } from "./__generated__/TicketListHarnessQuery.graphql";

const relayEndpoint = mswGraphql.link(getGraphQLEndpoint());
const server = setupServer();

beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

const harnessQuery = graphql`
  query TicketListHarnessQuery(
    $instanceId: ID!
    $status: TicketStatusFilterType!
    $assignedTo: ID
  ) @throwOnFieldError {
    ...TicketList_query
      @arguments(
        instanceId: $instanceId
        status: $status
        assignedTo: $assignedTo
      )
  }
`;

function ticketNode(overrides: {
  id: string;
  number: number;
  subject: string;
  status?: string;
}) {
  return {
    __typename: "Ticket",
    id: overrides.id,
    number: overrides.number,
    subject: overrides.subject,
    status: overrides.status ?? "OPEN",
    requesterEmails: ["customer@example.com"],
    lastActivityAt: Math.floor(Date.now() / 1000) - 3600,
    assignee: null,
    messages: [],
  };
}

let mountCount = 0;

function Harness({
  status,
  assignedTo,
}: {
  status: string;
  assignedTo: string | null;
}) {
  useEffect(() => {
    mountCount += 1;
  }, []);
  // Mirrors `TicketListPage`'s `Content`: the query's own variables are
  // frozen at mount, exactly once — a later `status` prop change must flow
  // through `TicketList`'s own refetch, not a second `useLazyLoadQuery`
  // fetch. See that component's doc comment.
  const [initialStatus] = useState(status);
  const data = useLazyLoadQuery<TicketListHarnessQuery>(harnessQuery, {
    instanceId: "inst-1",
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    status: initialStatus as any,
    assignedTo,
  });
  return (
    <TicketList
      query={data}
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      status={status as any}
      assignedTo={assignedTo}
      emptyMessage="Nothing here"
    />
  );
}

function renderHarness(initialStatus: string) {
  const environment = createUnauthenticatedGraphQLEnvironment();
  function Wrapper() {
    const [status, setStatus] = useState(initialStatus);
    return (
      <>
        <button onClick={() => setStatus("CLOSED")}>Switch to closed</button>
        <Suspense fallback="loading">
          <Harness status={status} assignedTo={null} />
        </Suspense>
      </>
    );
  }
  return render(
    <MemoryRouter>
      <RelayEnvironmentProvider environment={environment}>
        <Wrapper />
      </RelayEnvironmentProvider>
    </MemoryRouter>,
  );
}

describe("TicketList — pagination", () => {
  it("shows an empty state, then loads a second page past the boundary", async () => {
    server.use(
      relayEndpoint.query("TicketListHarnessQuery", () =>
        HttpResponse.json({
          data: {
            tickets: {
              edges: [
                {
                  node: ticketNode({
                    id: "t1",
                    number: 1,
                    subject: "First ticket",
                  }),
                  cursor: "c1",
                },
              ],
              pageInfo: {
                hasNextPage: true,
                hasPreviousPage: false,
                startCursor: "c1",
                endCursor: "c1",
              },
            },
          },
        }),
      ),
      relayEndpoint.query("TicketListPaginationQuery", ({ variables }) => {
        expect(variables.cursor).toBe("c1");
        return HttpResponse.json({
          data: {
            tickets: {
              edges: [
                {
                  node: ticketNode({
                    id: "t2",
                    number: 2,
                    subject: "Second ticket",
                  }),
                  cursor: "c2",
                },
              ],
              pageInfo: {
                hasNextPage: false,
                hasPreviousPage: true,
                startCursor: "c2",
                endCursor: "c2",
              },
            },
          },
        });
      }),
    );

    const user = UserEvent.setup();
    renderHarness("OPEN");

    expect(await screen.findByText("First ticket")).toBeInTheDocument();
    expect(screen.queryByText("Second ticket")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Load more" }));

    expect(await screen.findByText("Second ticket")).toBeInTheDocument();
    expect(screen.getByText("First ticket")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Load more" }),
    ).not.toBeInTheDocument();
  });

  it("renders the empty message when a filter has no tickets", async () => {
    server.use(
      relayEndpoint.query("TicketListHarnessQuery", () =>
        HttpResponse.json({
          data: {
            tickets: {
              edges: [],
              pageInfo: {
                hasNextPage: false,
                hasPreviousPage: false,
                startCursor: null,
                endCursor: null,
              },
            },
          },
        }),
      ),
    );

    renderHarness("OPEN");
    expect(await screen.findByText("Nothing here")).toBeInTheDocument();
  });
});

describe("TicketList — filter switching", () => {
  it("refetches on a filter change without remounting the component", async () => {
    let harnessCalls = 0;
    let paginationCalls = 0;
    server.use(
      relayEndpoint.query("TicketListHarnessQuery", ({ variables }) => {
        harnessCalls += 1;
        expect(variables.status).toBe("OPEN");
        return HttpResponse.json({
          data: {
            tickets: {
              edges: [
                {
                  node: ticketNode({
                    id: "t1",
                    number: 1,
                    subject: "Open ticket",
                  }),
                  cursor: "c1",
                },
              ],
              pageInfo: {
                hasNextPage: false,
                hasPreviousPage: false,
                startCursor: "c1",
                endCursor: "c1",
              },
            },
          },
        });
      }),
      relayEndpoint.query("TicketListPaginationQuery", ({ variables }) => {
        paginationCalls += 1;
        expect(variables.status).toBe("CLOSED");
        return HttpResponse.json({
          data: {
            tickets: {
              edges: [
                {
                  node: ticketNode({
                    id: "t2",
                    number: 2,
                    subject: "Closed ticket",
                    status: "CLOSED",
                  }),
                  cursor: "c2",
                },
              ],
              pageInfo: {
                hasNextPage: false,
                hasPreviousPage: false,
                startCursor: "c2",
                endCursor: "c2",
              },
            },
          },
        });
      }),
    );

    mountCount = 0;
    const user = UserEvent.setup();
    renderHarness("OPEN");

    expect(await screen.findByText("Open ticket")).toBeInTheDocument();
    await waitFor(() => expect(harnessCalls).toBe(1));
    expect(mountCount).toBe(1);

    await user.click(screen.getByRole("button", { name: "Switch to closed" }));

    expect(await screen.findByText("Closed ticket")).toBeInTheDocument();
    expect(screen.queryByText("Open ticket")).not.toBeInTheDocument();

    // The initial query (`TicketListHarnessQuery`) only ran once — the
    // filter change went through the pagination fragment's own `refetch`,
    // not a second full query fetch, and the component never remounted.
    expect(harnessCalls).toBe(1);
    expect(paginationCalls).toBe(1);
    expect(mountCount).toBe(1);
  });
});
