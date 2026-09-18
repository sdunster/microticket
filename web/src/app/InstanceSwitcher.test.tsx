import { Suspense, useState } from "react";
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
import { getGraphQLEndpoint } from "../lib/api";
import { createUnauthenticatedGraphQLEnvironment } from "../lib/environments";
import { InstanceSwitcher } from "./InstanceSwitcher";
import type { SelectedInstance } from "./selectedInstance";
import type { InstanceSwitcherTestQuery } from "./__generated__/InstanceSwitcherTestQuery.graphql";

const relayEndpoint = mswGraphql.link(getGraphQLEndpoint());
const server = setupServer();

beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => {
  server.resetHandlers();
  localStorage.clear();
});
afterAll(() => server.close());

const harnessQuery = graphql`
  query InstanceSwitcherTestQuery @throwOnFieldError {
    me {
      ...InstanceSwitcher_user
    }
  }
`;

function meResponse() {
  return {
    data: {
      me: {
        id: "user-1",
        memberships: [
          {
            role: "OWNER",
            instance: { id: "inst-acme", name: "Acme Support", slug: "acme" },
          },
          {
            role: "AGENT",
            instance: {
              id: "inst-ridge",
              name: "Ridgeline Help Desk",
              slug: "ridgeline",
            },
          },
        ],
      },
    },
  };
}

function Harness() {
  const data = useLazyLoadQuery<InstanceSwitcherTestQuery>(harnessQuery, {});
  const [selected, setSelected] = useState<SelectedInstance | null>(null);
  return (
    <InstanceSwitcher
      user={data.me}
      selected={selected}
      onChange={setSelected}
    />
  );
}

function renderHarness() {
  const environment = createUnauthenticatedGraphQLEnvironment();
  return render(
    <RelayEnvironmentProvider environment={environment}>
      <Suspense fallback="loading">
        <Harness />
      </Suspense>
    </RelayEnvironmentProvider>,
  );
}

describe("InstanceSwitcher", () => {
  it("defaults to the first membership when nothing is stored", async () => {
    server.use(
      relayEndpoint.query("InstanceSwitcherTestQuery", () =>
        HttpResponse.json(meResponse()),
      ),
    );
    renderHarness();

    const select = await screen.findByRole("combobox", { name: "Instance" });
    await waitFor(() => expect(select).toHaveValue("inst-acme"));
  });

  it("persists a selection across a remount (reload)", async () => {
    server.use(
      relayEndpoint.query("InstanceSwitcherTestQuery", () =>
        HttpResponse.json(meResponse()),
      ),
    );
    const user = UserEvent.setup();
    const { unmount } = renderHarness();

    const select = await screen.findByRole("combobox", { name: "Instance" });
    await waitFor(() => expect(select).toHaveValue("inst-acme"));

    await user.selectOptions(select, "inst-ridge");
    expect(select).toHaveValue("inst-ridge");

    unmount();

    renderHarness();
    const secondSelect = await screen.findByRole("combobox", {
      name: "Instance",
    });
    await waitFor(() => expect(secondSelect).toHaveValue("inst-ridge"));
  });
});
