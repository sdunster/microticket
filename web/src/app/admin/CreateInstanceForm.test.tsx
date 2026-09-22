import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import UserEvent from "@testing-library/user-event";
import { setupServer } from "msw/node";
import { graphql as mswGraphql, HttpResponse } from "msw";
import { RelayEnvironmentProvider } from "react-relay";
import { MemoryRouter } from "react-router";
import { getGraphQLEndpoint } from "../../lib/api";
import { createUnauthenticatedGraphQLEnvironment } from "../../lib/environments";
import { CreateInstanceForm } from "./CreateInstanceForm";

const relayEndpoint = mswGraphql.link(getGraphQLEndpoint());
const server = setupServer();

beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

function renderForm() {
  const environment = createUnauthenticatedGraphQLEnvironment();
  return render(
    <MemoryRouter>
      <RelayEnvironmentProvider environment={environment}>
        <CreateInstanceForm />
      </RelayEnvironmentProvider>
    </MemoryRouter>,
  );
}

describe("CreateInstanceForm", () => {
  it("submits the entered fields and surfaces a taken-slug error inline", async () => {
    let receivedVariables: Record<string, unknown> | undefined;
    server.use(
      relayEndpoint.mutation("CreateInstanceFormMutation", ({ variables }) => {
        receivedVariables = variables;
        return HttpResponse.json({
          data: null,
          errors: [{ message: 'slug "acme" is already taken' }],
        });
      }),
    );

    const user = UserEvent.setup();
    renderForm();

    await user.type(screen.getByLabelText("Name"), "Acme Support");
    await user.type(screen.getByLabelText("Slug"), "acme");
    await user.click(
      screen.getByRole("checkbox", { name: "Public submission enabled" }),
    );
    await user.click(screen.getByRole("button", { name: "Create instance" }));

    expect(
      await screen.findByText('slug "acme" is already taken'),
    ).toBeInTheDocument();

    expect(receivedVariables).toMatchObject({
      name: "Acme Support",
      slug: "acme",
      publicSubmissionEnabled: true,
    });
  });
});
