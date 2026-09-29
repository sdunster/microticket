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
import { RelayEnvironmentProvider } from "react-relay";
import { MemoryRouter } from "react-router";
import { getGraphQLEndpoint } from "../lib/api";
import { createUnauthenticatedGraphQLEnvironment } from "../lib/environments";
import { CurrentUserContext } from "../auth/CurrentUserContext";
import type { CurrentUserContextType } from "../auth/CurrentUserContext";
import OAuthAuthorize from "./OAuthAuthorize";

const relayEndpoint = mswGraphql.link(getGraphQLEndpoint());
const server = setupServer();

beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

const assign = vi.fn();
beforeEach(() => {
  assign.mockReset();
  Object.defineProperty(window, "location", {
    configurable: true,
    value: { ...window.location, assign },
  });
});

const REDIRECT = "https://client.example/callback";
const validQuery = new URLSearchParams({
  response_type: "code",
  client_id: "client-1",
  redirect_uri: REDIRECT,
  code_challenge: "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
  code_challenge_method: "S256",
  state: "xyz",
  resource: "https://toolbox.test/mcp",
});

const user = { email: "me@example.com" } as unknown as CurrentUserContextType;

function renderPage(search: URLSearchParams) {
  const environment = createUnauthenticatedGraphQLEnvironment();
  return render(
    <MemoryRouter initialEntries={[`/app/oauth/authorize?${search}`]}>
      <RelayEnvironmentProvider environment={environment}>
        <CurrentUserContext.Provider value={user}>
          <OAuthAuthorize />
        </CurrentUserContext.Provider>
      </RelayEnvironmentProvider>
    </MemoryRouter>,
  );
}

function serveConsentQuery() {
  server.use(
    relayEndpoint.query("OAuthAuthorizeQuery", () =>
      HttpResponse.json({
        data: {
          oauthAuthorizationRequest: {
            clientName: "Test Client",
            redirectHost: "client.example",
          },
        },
      }),
    ),
  );
}

describe("OAuthAuthorize", () => {
  it("shows the client, the redirect host and who it acts as", async () => {
    serveConsentQuery();
    renderPage(validQuery);
    expect(await screen.findByText("Connect Test Client?")).toBeInTheDocument();
    expect(screen.getByText("client.example")).toBeInTheDocument();
    expect(screen.getByText("me@example.com")).toBeInTheDocument();
  });

  it("approves by calling the mutation and following the returned URL", async () => {
    serveConsentQuery();
    let variables: Record<string, unknown> | undefined;
    server.use(
      relayEndpoint.mutation("OAuthAuthorizeMutation", (info) => {
        variables = info.variables;
        return HttpResponse.json({
          data: { approveOauthAuthorization: `${REDIRECT}?code=abc&state=xyz` },
        });
      }),
    );
    renderPage(validQuery);
    await UserEvent.setup().click(
      await screen.findByRole("button", { name: "Approve" }),
    );
    await vi.waitFor(() =>
      expect(assign).toHaveBeenCalledWith(`${REDIRECT}?code=abc&state=xyz`),
    );
    expect(variables).toMatchObject({
      clientId: "client-1",
      redirectUri: REDIRECT,
      codeChallengeMethod: "S256",
      state: "xyz",
      resource: "https://toolbox.test/mcp",
    });
  });

  it("denies by redirecting with access_denied and never calls the mutation", async () => {
    serveConsentQuery(); // no mutation handler: onUnhandledRequest would error
    renderPage(validQuery);
    await UserEvent.setup().click(
      await screen.findByRole("button", { name: "Deny" }),
    );
    expect(assign).toHaveBeenCalledWith(
      `${REDIRECT}?error=access_denied&state=xyz`,
    );
  });

  it("shows the server's message when approval fails, and doesn't navigate", async () => {
    serveConsentQuery();
    server.use(
      relayEndpoint.mutation("OAuthAuthorizeMutation", () =>
        HttpResponse.json({
          data: null,
          errors: [
            { message: "redirect_uri is not registered for this client" },
          ],
        }),
      ),
    );
    renderPage(validQuery);
    await UserEvent.setup().click(
      await screen.findByRole("button", { name: "Approve" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "redirect_uri is not registered for this client",
    );
    expect(assign).not.toHaveBeenCalled();
  });

  it("rejects a request missing required parameters without any network call", () => {
    const incomplete = new URLSearchParams({ client_id: "client-1" });
    renderPage(incomplete);
    expect(screen.getByText("Invalid request")).toBeInTheDocument();
  });

  it("requires response_type=code", () => {
    const wrong = new URLSearchParams(validQuery);
    wrong.set("response_type", "token");
    renderPage(wrong);
    expect(screen.getByText("Invalid request")).toBeInTheDocument();
  });
});
