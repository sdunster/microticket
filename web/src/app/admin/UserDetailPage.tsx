import { Suspense } from "react";
import { graphql } from "react-relay";
import { useParams } from "react-router";
import type { UserDetailPageQuery } from "./__generated__/UserDetailPageQuery.graphql";
import { useRetryableLazyLoadQuery } from "../../components/useRetryableLazyLoadQuery";
import RelayErrorBoundary from "../../components/RelayErrorBoundary";
import LoadingIndicator from "../../components/LoadingIndicator";
import { ButtonLink } from "../../components/ui/Button";
import { UserEditForm } from "./UserEditForm";
import { UserDeleteControl } from "./UserDeleteControl";
import { UserMembershipsSection } from "./UserMembershipsSection";

const userDetailPageQuery = graphql`
  query UserDetailPageQuery($id: ID!) @throwOnFieldError {
    adminUser(id: $id) {
      id
      ...UserEditForm_user
      ...UserDeleteControl_user
      ...UserMembershipsSection_user
    }
  }
`;

function Content({ id }: { id: string }) {
  const data = useRetryableLazyLoadQuery<UserDetailPageQuery>(
    userDetailPageQuery,
    { id },
  );

  if (!data.adminUser) {
    return (
      <div className="rounded-lg border border-dashed border-line p-10 text-center text-ink-muted">
        <p className="font-medium text-ink">User not found</p>
        <ButtonLink to="/app/admin/users" variant="secondary" className="mt-4">
          Back to users
        </ButtonLink>
      </div>
    );
  }

  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <ButtonLink
        to="/app/admin/users"
        variant="ghost"
        className="self-start px-0"
      >
        ← Back to users
      </ButtonLink>
      <UserEditForm user={data.adminUser} />
      <UserMembershipsSection user={data.adminUser} />
      <UserDeleteControl user={data.adminUser} />
    </div>
  );
}

/** `/app/admin/users/:id` — edit, memberships, and delete for one user. */
export function UserDetailPage() {
  const { id } = useParams<{ id: string }>();
  if (!id) return null;

  return (
    <RelayErrorBoundary canRetry>
      <Suspense fallback={<LoadingIndicator />}>
        <Content id={id} />
      </Suspense>
    </RelayErrorBoundary>
  );
}
