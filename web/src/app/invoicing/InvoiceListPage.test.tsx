import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import UserEvent from "@testing-library/user-event";
import { setupServer } from "msw/node";
import { graphql as mswGraphql, HttpResponse } from "msw";
import { RelayEnvironmentProvider } from "react-relay";
import { MemoryRouter } from "react-router";
import { getGraphQLEndpoint } from "../../lib/api";
import { createUnauthenticatedGraphQLEnvironment } from "../../lib/environments";
import { SelectedInstanceContext } from "../SelectedInstanceContext";
import type { SelectedInstance } from "../selectedInstance";
import { InvoiceListPage } from "./InvoiceListPage";

const relayEndpoint = mswGraphql.link(getGraphQLEndpoint());
const server = setupServer();

beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

const instance: SelectedInstance = {
  id: "inst-ledger",
  slug: "ledger",
  name: "Ledger & Co Bookkeeping",
  role: "OWNER",
  kind: "INVOICING",
};

function invoiceNode(overrides: Partial<ReturnType<typeof draftInvoice>> = {}) {
  return { ...draftInvoice(), ...overrides };
}

function draftInvoice() {
  return {
    __typename: "Invoice",
    id: "inv-draft",
    status: "DRAFT",
    displayNumber: null as string | null,
    issueDate: null as string | null,
    paidDate: null as string | null,
    totalCents: 150000,
    currency: "AUD",
    project: { __typename: "Project", id: "proj-1", name: "Website Redesign" },
  };
}

function connection(nodes: Array<ReturnType<typeof invoiceNode>>) {
  return {
    edges: nodes.map((node) => ({ node, cursor: node.id })),
    pageInfo: {
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: null,
      endCursor: null,
    },
  };
}

function renderPage() {
  const environment = createUnauthenticatedGraphQLEnvironment();
  return render(
    <MemoryRouter initialEntries={["/app/invoices"]}>
      <RelayEnvironmentProvider environment={environment}>
        <SelectedInstanceContext value={instance}>
          <InvoiceListPage />
        </SelectedInstanceContext>
      </RelayEnvironmentProvider>
    </MemoryRouter>,
  );
}

describe("InvoiceListPage", () => {
  it("renders the all-invoices list and its rows", async () => {
    server.use(
      relayEndpoint.query("InvoiceListPageQuery", ({ variables }) => {
        expect(variables.filter).toBe("ALL");
        return HttpResponse.json({
          data: {
            invoices: connection([
              invoiceNode(),
              invoiceNode({
                id: "inv-final",
                status: "FINALIZED",
                displayNumber: "008",
                issueDate: "2026-08-19",
                totalCents: 275000,
              }),
            ]),
          },
        });
      }),
    );
    renderPage();

    // The draft row shows "Draft" twice — once as its number, once as its
    // status badge — so assert on the row rather than a unique text match.
    const rows = await screen.findAllByRole("listitem");
    expect(rows).toHaveLength(2);
    expect(within(rows[0]).getAllByText("Draft")).toHaveLength(2);
    expect(within(rows[0]).getByRole("link")).toHaveAttribute(
      "href",
      "/app/invoices/inv-draft",
    );
    expect(within(rows[1]).getByText("008")).toBeInTheDocument();
    expect(within(rows[1]).getByText("19/08/2026")).toBeInTheDocument();
    expect(within(rows[1]).getByText("Website Redesign")).toBeInTheDocument();
  });

  it("refetches with the selected filter instead of re-running the page query", async () => {
    let pageCalls = 0;
    let refetchFilter: unknown;
    server.use(
      relayEndpoint.query("InvoiceListPageQuery", ({ variables }) => {
        pageCalls += 1;
        expect(variables.filter).toBe("ALL");
        return HttpResponse.json({
          data: { invoices: connection([invoiceNode()]) },
        });
      }),
      relayEndpoint.query("InvoiceListPaginationQuery", ({ variables }) => {
        refetchFilter = variables.filter;
        return HttpResponse.json({
          data: {
            invoices: connection([
              invoiceNode({
                id: "inv-unpaid",
                status: "FINALIZED",
                displayNumber: "009",
                issueDate: "2026-08-20",
              }),
            ]),
          },
        });
      }),
    );
    const user = UserEvent.setup();
    renderPage();

    await screen.findAllByRole("listitem");
    await user.click(screen.getByRole("button", { name: "Unpaid" }));

    await waitFor(() => expect(refetchFilter).toBe("UNPAID"));
    expect(await screen.findByText("009")).toBeInTheDocument();
    expect(pageCalls).toBe(1);
  });

  it("shows the filter's own empty message", async () => {
    server.use(
      relayEndpoint.query("InvoiceListPageQuery", () =>
        HttpResponse.json({ data: { invoices: connection([]) } }),
      ),
    );
    renderPage();
    expect(await screen.findByText("No invoices yet.")).toBeInTheDocument();
  });

  it("marks the active filter tab pressed", async () => {
    server.use(
      relayEndpoint.query("InvoiceListPageQuery", () =>
        HttpResponse.json({ data: { invoices: connection([invoiceNode()]) } }),
      ),
    );
    renderPage();
    await screen.findAllByRole("listitem");
    const tabs = within(screen.getByRole("group", { name: "Filter invoices" }));
    expect(tabs.getByRole("button", { name: "All" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(tabs.getByRole("button", { name: "Draft" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });
});
