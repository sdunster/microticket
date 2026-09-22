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
import { CurrentUserContext } from "../../auth/CurrentUserContext";
import { UserEditForm } from "./UserEditForm";
import { UserDeleteControl } from "./UserDeleteControl";
import type { UserDetailSelfHarnessQuery } from "./__generated__/UserDetailSelfHarnessQuery.graphql";

const fakeCurrentUser = { id: "user-1" } as never;

const relayEndpoint = mswGraphql.link(getGraphQLEndpoint());
const server = setupServer();

beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

const harnessQuery = graphql`
  query UserDetailSelfHarnessQuery($id: ID!) @throwOnFieldError {
    adminUser(id: $id) {
      ...UserEditForm_user
      ...UserDeleteControl_user
    }
  }
`;

function targetUser(id: string) {
  return {
    __typename: "User",
    id,
    name: "Adrian Agent",
    email: "agent@example.com",
    enabled: true,
    isSuperuser: false,
  };
}

function Harness({ id }: { id: string }) {
  const data = useLazyLoadQuery<UserDetailSelfHarnessQuery>(harnessQuery, {
    id,
  });
  if (!data.adminUser) return null;
  return (
    <>
      <UserEditForm user={data.adminUser} />
      <UserDeleteControl user={data.adminUser} />
    </>
  );
}

function renderHarness(id: string) {
  const environment = createUnauthenticatedGraphQLEnvironment();
  return render(
    <MemoryRouter>
      <CurrentUserContext value={fakeCurrentUser}>
        <RelayEnvironmentProvider environment={environment}>
          <Suspense fallback="loading">
            <Harness id={id} />
          </Suspense>
        </RelayEnvironmentProvider>
      </CurrentUserContext>
    </MemoryRouter>,
  );
}

describe("UserEditForm / UserDeleteControl — self record", () => {
  it("hides the enabled toggle and delete button on the caller's own record", async () => {
    server.use(
      relayEndpoint.query("UserDetailSelfHarnessQuery", () =>
        HttpResponse.json({ data: { adminUser: targetUser("user-1") } }),
      ),
    );

    renderHarness("user-1");

    await screen.findByDisplayValue("Adrian Agent");
    expect(screen.queryByLabelText("Enabled")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Delete user" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText("You can't delete your own account."),
    ).toBeInTheDocument();
  });

  it("shows both controls for a different user's record", async () => {
    server.use(
      relayEndpoint.query("UserDetailSelfHarnessQuery", () =>
        HttpResponse.json({ data: { adminUser: targetUser("user-2") } }),
      ),
    );

    renderHarness("user-2");

    await screen.findByDisplayValue("Adrian Agent");
    expect(screen.getByLabelText("Enabled")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Delete user" }),
    ).toBeInTheDocument();
  });
});
