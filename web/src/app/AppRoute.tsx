import { Suspense } from "react";
import { Navigate, Route, Routes } from "react-router";
import AuthenticatedSession from "../auth/AuthenticatedSession";
import { useCurrentUser } from "../auth/useCurrentUser";
import { lazyWithReload } from "../lib/lazyWithReload";
import LoadingIndicator from "../components/LoadingIndicator";
import AppShell from "./AppShell";
import Settings from "./Settings";
import { TicketListPage } from "./tickets/TicketListPage";
import { TicketThreadPage } from "./tickets/TicketThreadPage";

// Its own lazy chunk, loaded only once someone actually navigates under
// `/app/admin/*` — most users are never superusers and never need this
// code, per the same reasoning `Router.tsx` already applies to `/app/*`
// itself, `/login`, and `/submit`.
const AdminRoute = lazyWithReload("admin", () => import("./admin/AdminRoute"));

/**
 * `/app`'s index route. A superuser with zero memberships has nothing to
 * land on at `tickets/open` — that page just shows "pick an instance"
 * forever, since a superuser gets no ticket access without a real
 * membership (see `CLAUDE.md`'s superuser boundary). Send them to the admin
 * area instead, where they actually have something to do. Anyone else
 * (including a superuser who *is* a member somewhere) keeps the original
 * behavior unchanged.
 */
function AppIndexRedirect() {
  const user = useCurrentUser();
  if (user.isSuperuser && user.memberships.length === 0) {
    return <Navigate to="admin" replace />;
  }
  return <Navigate to="tickets/open" replace />;
}

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
          <Route index element={<AppIndexRedirect />} />
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
          <Route
            path="admin/*"
            element={
              // A local boundary, not the top-level Router one, so only the
              // content area suspends while the admin chunk loads — the
              // shell (nav, instance switcher) stays put instead of
              // momentarily disappearing.
              <Suspense fallback={<LoadingIndicator />}>
                <AdminRoute />
              </Suspense>
            }
          />
        </Route>
      </Routes>
    </AuthenticatedSession>
  );
}
