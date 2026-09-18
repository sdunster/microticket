import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import UserEvent from "@testing-library/user-event";
import { setupServer } from "msw/node";
import { graphql, HttpResponse } from "msw";
import { getGraphQLEndpoint } from "../lib/api";
import { SubmitForm } from "./SubmitForm";

const relayEndpoint = graphql.link(getGraphQLEndpoint());
const server = setupServer();

beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe("SubmitForm — code step", () => {
  it("rejects a wrong code and lets the requester retry", async () => {
    server.use(
      relayEndpoint.mutation("RequestSubmitCode", () =>
        HttpResponse.json({ data: { requestSubmitCode: true } }),
      ),
      relayEndpoint.mutation("VerifySubmitCode", () =>
        HttpResponse.json({ data: { verifySubmitCode: null } }),
      ),
    );

    const user = UserEvent.setup();
    render(<SubmitForm slug="acme" />);

    await user.type(
      screen.getByLabelText("Email address"),
      "customer@example.com",
    );
    await user.click(screen.getByRole("button", { name: "Send code" }));

    const codeInput = await screen.findByLabelText("6-digit code");
    await user.type(codeInput, "000000");
    await user.click(screen.getByRole("button", { name: "Verify code" }));

    expect(
      await screen.findByText("Incorrect or expired code. Please try again."),
    ).toBeInTheDocument();
    // Back on the code screen, ready to retry, with the wrong code cleared —
    // not dumped onto a ticket form it never earned.
    expect(screen.getByLabelText("6-digit code")).toHaveValue("");
    expect(screen.queryByLabelText("Subject")).not.toBeInTheDocument();
  });

  it("reaches the ticket form on a correct code", async () => {
    server.use(
      relayEndpoint.mutation("RequestSubmitCode", () =>
        HttpResponse.json({ data: { requestSubmitCode: true } }),
      ),
      relayEndpoint.mutation("VerifySubmitCode", () =>
        HttpResponse.json({ data: { verifySubmitCode: "mts_abc123" } }),
      ),
    );

    const user = UserEvent.setup();
    render(<SubmitForm slug="acme" />);

    await user.type(
      screen.getByLabelText("Email address"),
      "customer@example.com",
    );
    await user.click(screen.getByRole("button", { name: "Send code" }));

    const codeInput = await screen.findByLabelText("6-digit code");
    await user.type(codeInput, "123456");
    await user.click(screen.getByRole("button", { name: "Verify code" }));

    expect(await screen.findByLabelText("Subject")).toBeInTheDocument();
  });
});
