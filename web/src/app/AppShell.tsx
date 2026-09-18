import { useState } from "react";
import { NavLink, Outlet } from "react-router";
import { useCurrentUser } from "../auth/useCurrentUser";
import { useLogout } from "../auth/useLogout";
import { Button } from "../components/ui/Button";
import { InstanceSwitcher } from "./InstanceSwitcher";
import { SelectedInstanceContext } from "./SelectedInstanceContext";
import type { SelectedInstance } from "./selectedInstance";

// Absolute paths: a bare "settings" resolves relative to whichever child
// route is currently active (e.g. `/app/tickets/open` → `/app/tickets/open/settings`),
// not to this shell's own level — verified against the running dev server.
const NAV_ITEMS = [
  { to: "/app/tickets/open", label: "Open" },
  { to: "/app/tickets/closed", label: "Closed" },
  { to: "/app/tickets/all", label: "All" },
  { to: "/app/tickets/mine", label: "Mine" },
  { to: "/app/settings", label: "Settings" },
];

const navLinkClass = ({ isActive }: { isActive: boolean }) =>
  [
    "rounded-md px-3 py-1.5 text-sm font-medium transition-colors",
    isActive
      ? "bg-accent/15 text-accent-dark dark:text-accent-light"
      : "text-ink-muted hover:bg-surface-sunken hover:text-ink",
  ].join(" ");

/**
 * The authenticated shell: header (instance switcher, current user, log
 * out) plus the Open/Closed/All/Mine/Settings nav. Child routes render into
 * the `<Outlet/>` — ticket lists and the thread view land in step 8b; only
 * `/app/settings` is real in this step.
 */
export default function AppShell() {
  const user = useCurrentUser();
  const logout = useLogout();
  const [selected, setSelected] = useState<SelectedInstance | null>(null);

  return (
    <SelectedInstanceContext value={selected}>
      <div className="flex min-h-screen flex-col bg-surface">
        <header className="flex flex-wrap items-center justify-between gap-3 border-b border-line px-4 py-3 sm:px-6">
          <div className="flex items-center gap-4">
            <span className="text-lg font-semibold text-ink-strong">
              microticket
            </span>
            <InstanceSwitcher
              user={user}
              selected={selected}
              onChange={setSelected}
            />
          </div>
          <div className="flex items-center gap-3 text-sm text-ink-muted">
            <span>{user.email}</span>
            <Button variant="ghost" onClick={logout}>
              Log out
            </Button>
          </div>
        </header>

        <nav className="flex flex-wrap gap-1 border-b border-line px-4 py-2 sm:px-6">
          {NAV_ITEMS.map((item) => (
            <NavLink key={item.to} to={item.to} className={navLinkClass}>
              {item.label}
            </NavLink>
          ))}
        </nav>

        <main className="flex-1 px-4 py-6 sm:px-6">
          <Outlet />
        </main>
      </div>
    </SelectedInstanceContext>
  );
}
