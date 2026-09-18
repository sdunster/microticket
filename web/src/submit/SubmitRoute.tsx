import { Link, useParams } from "react-router";
import { PublicInstanceList } from "./PublicInstanceList";
import { SubmitForm } from "./SubmitForm";

/**
 * `/submit` lists public instances (`PublicInstanceList`); `/submit/:slug`
 * is the actual email → code → subject/body flow (`SubmitForm`). Neither
 * requires a session — see the build plan's "public submit form" section.
 */
export default function SubmitRoute() {
  const { slug } = useParams();
  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-4 bg-surface px-4 py-12">
      {slug ? <SubmitForm slug={slug} /> : <PublicInstanceList />}
      <p className="text-xs text-ink-muted">
        Staff:{" "}
        <Link to="/login" className="underline">
          log in
        </Link>{" "}
        instead.
      </p>
    </div>
  );
}
