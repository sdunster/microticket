import { NavLink, Navigate, Outlet, Route, Routes } from "react-router";
import { InstanceListPage } from "./InstanceListPage";
import { InstanceDetailPage } from "./InstanceDetailPage";
import { UserListPage } from "./UserListPage";
import { UserDetailPage } from "./UserDetailPage";

const navLinkClass = ({ isActive }: { isActive: boolean }) =>
  [
    "rounded-md px-3 py-1.5 text-sm font-medium transition-colors",
    isActive
      ? "bg-accent/15 text-accent-dark dark:text-accent-light"
      : "text-ink-muted hover:bg-surface-sunken hover:text-ink",
  ].join(" ");

/**
 * A second-level nav for the two admin sections, mirroring `AppShell`'s own
 * nav bar one level down. `AppShell` itself already gates the top-level
 * "Admin" link to superusers; the API is the real boundary for everything
 * under here regardless (see `AppRoute`'s `AdminRoute` comment).
 */
function AdminShell() {
  return (
    <div className="flex flex-col gap-6">
      <nav className="flex flex-wrap gap-1 border-b border-line pb-2">
        <NavLink to="/app/admin/instances" className={navLinkClass}>
          Instances
        </NavLink>
        <NavLink to="/app/admin/users" className={navLinkClass}>
          Users
        </NavLink>
      </nav>
      <Outlet />
    </div>
  );
}

/**
 * `/app/admin/*` — instance and user administration. Its own lazily-loaded
 * chunk (see `AppRoute`), since most users are never superusers. Nested
 * routing lives here rather than in `AppRoute` for the same reason
 * `AppRoute`'s own routing doesn't live in the top-level `Router`: this
 * whole subtree is already one chunk.
 */
export default function AdminRoute() {
  return (
    <Routes>
      <Route element={<AdminShell />}>
        <Route index element={<Navigate to="instances" replace />} />
        <Route path="instances" element={<InstanceListPage />} />
        <Route path="instances/:id" element={<InstanceDetailPage />} />
        <Route path="users" element={<UserListPage />} />
        <Route path="users/:id" element={<UserDetailPage />} />
      </Route>
    </Routes>
  );
}
