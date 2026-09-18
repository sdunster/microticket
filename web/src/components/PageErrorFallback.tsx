import { useState } from "react";
import type { FallbackProps } from "react-error-boundary";
import { Button } from "./ui/Button";

interface PageErrorFallbackProps extends FallbackProps {
  showDetailsByDefault?: boolean;
  /**
   * Show "Reload page" (a real `window.location.reload()`) instead of "Try
   * again" (`resetErrorBoundary`). Use this wherever the boundary can't
   * guarantee a retry actually refetches — see `RelayErrorBoundary`'s
   * `canRetry` — since a full reload always recovers, but a boundary reset
   * can silently redisplay the exact same cached error.
   */
  reloadInstead?: boolean;
}

export default function PageErrorFallback({
  error,
  resetErrorBoundary,
  showDetailsByDefault = false,
  reloadInstead = false,
}: PageErrorFallbackProps) {
  const [showDetails, setShowDetails] = useState(showDetailsByDefault);
  const message = error instanceof Error ? error.message : String(error);

  return (
    <div
      role="alert"
      className="flex flex-col items-center gap-3 p-8 text-center"
    >
      <p className="text-ink-strong">Something went wrong.</p>
      {showDetails ? (
        <pre className="max-w-full overflow-x-auto text-sm text-red-600 dark:text-red-400">
          {message}
        </pre>
      ) : null}
      <div className="flex flex-wrap justify-center gap-2">
        {reloadInstead ? (
          <Button onClick={() => window.location.reload()}>Reload page</Button>
        ) : (
          <Button onClick={resetErrorBoundary}>Try again</Button>
        )}
        <Button
          variant="secondary"
          onClick={() => setShowDetails((prev) => !prev)}
        >
          {showDetails ? "Hide details" : "Show details"}
        </Button>
      </div>
    </div>
  );
}
