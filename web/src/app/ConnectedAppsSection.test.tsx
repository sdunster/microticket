import { Suspense } from "react";
import {
  afterAll,
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import { render, screen } from "@testing-library/react";
import UserEvent from "@testing-library/user-event";
import { setupServer } from "msw/node";
import { graphql as mswGraphql, HttpResponse } from "msw";
import {
  graphql,
  useLazyLoadQuery,
  RelayEnvironmentProvider,
} from "react-relay";
import { getGraphQLEndpoint } from "../lib/api";
import { createUnauthenticatedGraphQLEnvironment } from "../lib/environments";
import { ConnectedAppsSection } from "./ConnectedAppsSection";
import type { ConnectedAppsSectionHarnessQuery } from "./__generated__/ConnectedAppsSectionHarnessQuery.graphql";

const relayEndpoint = mswGraphql.link(getGraphQLEndpoint());
const server = setupServer();

beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

const harnessQuery = graphql`
  query ConnectedAppsSectionHarnessQuery @throwOnFieldError {
    me {
      ...ConnectedAppsSection_user
    }
  }
`;

function Harness() {
  const data = useLazyLoadQuery<ConnectedAppsSectionHarnessQuery>(
    harnessQuery,
    {},
  );
  return <ConnectedAppsSection user={data.me} />;
}

function renderHarness() {
  return render(
    <RelayEnvironmentProvider
      environment={createUnauthenticatedGraphQLEnvironment()}
    >
      <Suspense fallback="loading">
        <Harness />
      </Suspense>
    </RelayEnvironmentProvider>,
  );
}

const now = Math.floor(Date.now() / 1000);
function grant(id: string, name: string, lastUsedAt: number | null) {
  return {
    __typename: "OAuthGrant",
    id,
    clientName: name,
    redirectHost: "client.example",
    createdAt: now - 3600,
    lastUsedAt,
    refreshExpiresAt: now + 86400 * 30,
  };
}
const me = (grants: ReturnType<typeof grant>[]) => ({
  __typename: "User",
  id: "u1",
  oauthGrants: grants,
});

beforeEach(() => {
  vi.spyOn(window, "confirm").mockReturnValue(true);
});
afterEach(() => vi.restoreAllMocks());

describe("ConnectedAppsSection", () => {
  it("says so when nothing is connected", async () => {
    server.use(
      relayEndpoint.query("ConnectedAppsSectionHarnessQuery", () =>
        HttpResponse.json({ data: { me: me([]) } }),
      ),
    );
    renderHarness();
    expect(
      await screen.findByText("No AI apps connected yet."),
    ).toBeInTheDocument();
  });

  it("lists each app with its redirect host and last-used state", async () => {
    server.use(
      relayEndpoint.query("ConnectedAppsSectionHarnessQuery", () =>
        HttpResponse.json({
          data: {
            me: me([
              grant("g1", "Claude", now - 60 * 60),
              grant("g2", "Cursor", null),
            ]),
          },
        }),
      ),
    );
    renderHarness();
    expect(await screen.findByText(/^Claude/)).toBeInTheDocument();
    expect(screen.getByText(/^Cursor/)).toBeInTheDocument();
    expect(screen.getAllByText(/redirects to client\.example/)).toHaveLength(2);
    expect(screen.getByText(/last used never/)).toBeInTheDocument();
  });

  it("disconnects an app after confirming, removing just that row", async () => {
    let revoked: unknown;
    server.use(
      relayEndpoint.query("ConnectedAppsSectionHarnessQuery", () =>
        HttpResponse.json({
          data: {
            me: me([grant("g1", "Claude", null), grant("g2", "Cursor", null)]),
          },
        }),
      ),
      relayEndpoint.mutation("ConnectedAppsSection_RevokeMutation", (info) => {
        revoked = info.variables;
        return HttpResponse.json({ data: { revokeOauthGrant: true } });
      }),
    );
    renderHarness();
    const buttons = await screen.findAllByRole("button", {
      name: "Disconnect",
    });
    await UserEvent.setup().click(buttons[0]);

    await vi.waitFor(() => expect(screen.queryByText(/^Claude/)).toBeNull());
    expect(revoked).toEqual({ id: "g1" });
    expect(screen.getByText(/^Cursor/)).toBeInTheDocument();
    expect(window.confirm).toHaveBeenCalledWith(
      expect.stringContaining('Disconnect "Claude"?'),
    );
  });

  it("does nothing when the confirmation is declined", async () => {
    vi.spyOn(window, "confirm").mockReturnValue(false);
    server.use(
      relayEndpoint.query("ConnectedAppsSectionHarnessQuery", () =>
        HttpResponse.json({ data: { me: me([grant("g1", "Claude", null)]) } }),
      ),
    ); // no mutation handler: an attempted revoke would fail the test
    renderHarness();
    await UserEvent.setup().click(
      await screen.findByRole("button", { name: "Disconnect" }),
    );
    expect(screen.getByText(/^Claude/)).toBeInTheDocument();
  });

  it("shows the server's message and keeps the row when revoking fails", async () => {
    server.use(
      relayEndpoint.query("ConnectedAppsSectionHarnessQuery", () =>
        HttpResponse.json({ data: { me: me([grant("g1", "Claude", null)]) } }),
      ),
      relayEndpoint.mutation("ConnectedAppsSection_RevokeMutation", () =>
        HttpResponse.json({
          data: null,
          errors: [{ message: "OAuth grant not found" }],
        }),
      ),
    );
    renderHarness();
    await UserEvent.setup().click(
      await screen.findByRole("button", { name: "Disconnect" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "OAuth grant not found",
    );
    expect(screen.getByText(/^Claude/)).toBeInTheDocument();
  });
});
