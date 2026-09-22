import { graphql, useFragment } from "react-relay";
import { Link } from "react-router";
import type { UserMembershipsSection_user$key } from "./__generated__/UserMembershipsSection_user.graphql";
import { Card } from "../../components/ui/Card";

const userMembershipsSectionFragment = graphql`
  fragment UserMembershipsSection_user on User {
    memberships {
      role
      instance {
        id
        name
        slug
      }
    }
  }
`;

/**
 * Read-only: which instances this user belongs to, and in what role, each
 * linking to that instance's own admin page. Membership itself is managed
 * from there (`MembersSection`), not here — one place owns add/remove/role,
 * same as the build plan's "member invites flow through the instance" shape.
 */
export function UserMembershipsSection({
  user,
}: {
  user: UserMembershipsSection_user$key;
}) {
  const data = useFragment(userMembershipsSectionFragment, user);

  return (
    <Card>
      <h2 className="mb-4 text-sm font-semibold tracking-wide text-ink-muted uppercase">
        Memberships
      </h2>
      {data.memberships.length === 0 ? (
        <p className="text-sm text-ink-muted">Not a member of any instance.</p>
      ) : (
        <ul className="flex flex-col divide-y divide-line-faint">
          {data.memberships.map((m) => (
            <li
              key={m.instance.id}
              className="flex items-center justify-between gap-2 py-2"
            >
              <Link
                to={`/app/admin/instances/${m.instance.id}`}
                className="text-sm text-ink underline"
              >
                {m.instance.name}
                <span className="ml-1 text-ink-muted">/{m.instance.slug}</span>
              </Link>
              <span className="text-xs text-ink-muted capitalize">
                {m.role.toLowerCase()}
              </span>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}
