import { Suspense } from "react";
import { graphql } from "react-relay";
import type { UserListPageQuery } from "./__generated__/UserListPageQuery.graphql";
import { useRetryableLazyLoadQuery } from "../../components/useRetryableLazyLoadQuery";
import RelayErrorBoundary from "../../components/RelayErrorBoundary";
import LoadingIndicator from "../../components/LoadingIndicator";
import { UserRow } from "./UserRow";
import { CreateUserForm } from "./CreateUserForm";

const userListPageQuery = graphql`
  query UserListPageQuery @throwOnFieldError {
    adminUsers {
      id
      ...UserRow_user
    }
  }
`;

function Content() {
  const data = useRetryableLazyLoadQuery<UserListPageQuery>(
    userListPageQuery,
    {},
  );

  return (
    <div className="flex max-w-2xl flex-col gap-8">
      <div>
        <h1 className="text-xl font-semibold text-ink-strong">Users</h1>
        <p className="mt-1 text-sm text-ink-muted">Every user account.</p>
      </div>

      <CreateUserForm />

      {data.adminUsers.length === 0 ? (
        <p className="text-sm text-ink-muted">No users yet.</p>
      ) : (
        <ul className="flex flex-col divide-y divide-line-faint rounded-lg border border-line">
          {data.adminUsers.map((user) => (
            <li key={user.id}>
              <UserRow user={user} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** `/app/admin/users` — every user account, plus the create form. */
export function UserListPage() {
  return (
    <RelayErrorBoundary canRetry>
      <Suspense fallback={<LoadingIndicator />}>
        <Content />
      </Suspense>
    </RelayErrorBoundary>
  );
}
