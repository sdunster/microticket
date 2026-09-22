import { graphql, useFragment } from "react-relay";
import { Link } from "react-router";
import type { UserRow_user$key } from "./__generated__/UserRow_user.graphql";

const userRowFragment = graphql`
  fragment UserRow_user on User {
    id
    name
    email
    isSuperuser
    enabled
  }
`;

/** One row of the admin user list, with badges for the two states that
 * matter at a glance: superuser and disabled. */
export function UserRow({ user }: { user: UserRow_user$key }) {
  const data = useFragment(userRowFragment, user);

  return (
    <Link
      to={`/app/admin/users/${data.id}`}
      className="flex flex-wrap items-center justify-between gap-2 p-3 no-underline transition-colors hover:bg-surface-raised"
    >
      <div className="flex items-center gap-2">
        <span className="font-medium text-ink-strong">{data.name}</span>
        <span className="text-sm text-ink-muted">{data.email}</span>
      </div>
      <div className="flex items-center gap-2">
        {data.isSuperuser && (
          <span className="rounded-full bg-accent/15 px-2 py-0.5 text-xs font-medium text-accent-dark dark:text-accent-light">
            Superuser
          </span>
        )}
        {!data.enabled && (
          <span className="rounded-full bg-red-100 px-2 py-0.5 text-xs font-medium text-red-800 dark:bg-red-900/40 dark:text-red-300">
            Disabled
          </span>
        )}
      </div>
    </Link>
  );
}
