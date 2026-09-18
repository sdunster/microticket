import { Navigate, Route, Routes } from "react-router";
import AuthenticatedSession from "../auth/AuthenticatedSession";
import AppShell from "./AppShell";
import Settings from "./Settings";
import { TicketsPlaceholder } from "./TicketsPlaceholder";

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
            element={<TicketsPlaceholder title="Open tickets" />}
          />
          <Route
            path="tickets/closed"
            element={<TicketsPlaceholder title="Closed tickets" />}
          />
          <Route
            path="tickets/all"
            element={<TicketsPlaceholder title="All tickets" />}
          />
          <Route
            path="tickets/mine"
            element={<TicketsPlaceholder title="Assigned to me" />}
          />
          <Route
            path="tickets/:id"
            element={<TicketsPlaceholder title="Ticket" />}
          />
          <Route path="settings" element={<Settings />} />
        </Route>
      </Routes>
    </AuthenticatedSession>
  );
}
