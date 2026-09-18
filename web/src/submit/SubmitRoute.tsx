import { Link, useParams } from "react-router";
import { Card } from "../components/ui/Card";
import { ButtonLink } from "../components/ui/Button";

/**
 * Placeholder for `/submit` (list public instances) and `/submit/:slug`
 * (email → code → subject/body). Step 8b builds the real form; this exists
 * only so the "Submit a new ticket" door on `/` leads somewhere real.
 */
export default function SubmitRoute() {
  const { slug } = useParams();
  return (
    <div className="flex min-h-screen items-center justify-center bg-surface px-4 py-12">
      <Card className="w-full max-w-md text-center">
        <h1 className="text-xl font-semibold text-ink-strong">
          Submit a ticket
        </h1>
        <p className="mt-2 text-sm text-ink-muted">
          {slug
            ? `The public submission form for "${slug}" isn't built yet.`
            : "The public submission form isn't built yet."}{" "}
          It lands in the next step of the build.
        </p>
        <ButtonLink to="/" variant="secondary" className="mt-4">
          Back to start
        </ButtonLink>
        <p className="mt-4 text-xs text-ink-muted">
          Staff:{" "}
          <Link to="/login" className="underline">
            log in
          </Link>{" "}
          instead.
        </p>
      </Card>
    </div>
  );
}
