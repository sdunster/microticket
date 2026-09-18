import { Navigate, Route, Routes } from "react-router";
import AuthenticatedSession from "../auth/AuthenticatedSession";
import AppShell from "./AppShell";
import Settings from "./Settings";
import { TicketListPage } from "./tickets/TicketListPage";
import { TicketThreadPage } from "./tickets/TicketThreadPage";

/**
 * `/app/*` — the authenticated area. `AuthenticatedSession` decides between
 * the login page and this tree; everything below assumes a valid session.
 * Nested routing lives here (not in the top-level `Router`) since this
 * whole subtree is one lazy chunk already.
 */
export default function AppRoute() {
  return (
    <AuthenticatedSession>
      <Routes>
        <Route element={<AppShell />}>
          <Route index element={<Navigate to="tickets/open" replace />} />
          <Route
            path="tickets/open"
            element={
              <TicketListPage
                status="OPEN"
                title="Open tickets"
                emptyMessage="No open tickets — the queue is clear."
              />
            }
          />
          <Route
            path="tickets/closed"
            element={
              <TicketListPage
                status="CLOSED"
                title="Closed tickets"
                emptyMessage="No closed tickets yet."
              />
            }
          />
          <Route
            path="tickets/all"
            element={
              <TicketListPage
                status="ALL"
                title="All tickets"
                emptyMessage="No tickets yet."
              />
            }
          />
          <Route
            path="tickets/mine"
            element={
              <TicketListPage
                status="ALL"
                assignedToMe
                title="Assigned to me"
                emptyMessage="Nothing assigned to you right now."
              />
            }
          />
          <Route
            path="tickets/deleted"
            element={
              <TicketListPage
                status="DELETED"
                title="Deleted tickets"
                emptyMessage="No deleted tickets."
              />
            }
          />
          <Route path="tickets/:id" element={<TicketThreadPage />} />
          <Route path="settings" element={<Settings />} />
        </Route>
      </Routes>
    </AuthenticatedSession>
  );
}
