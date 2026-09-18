import { Suspense } from "react";
import { BrowserRouter, Routes, Route } from "react-router";
import { ErrorBoundary } from "react-error-boundary";

import { lazyWithReload } from "./lib/lazyWithReload";
import LoadingIndicator from "./components/LoadingIndicator";
import PageErrorFallback from "./components/PageErrorFallback";

// Home is the front door — keep it eager for fast first paint.
import Home from "./home/Home";

// Everything else is its own lazily-loaded chunk.
const LoginRoute = lazyWithReload("login", () => import("./auth/LoginRoute"));
const AppRoute = lazyWithReload("app", () => import("./app/AppRoute"));
const SubmitRoute = lazyWithReload(
  "submit",
  () => import("./submit/SubmitRoute"),
);

export default function Router() {
  return (
    <BrowserRouter>
      {/* Backstop for anything that escapes a more specific boundary
          further down the tree (e.g. AuthenticatedSession's own). */}
      <ErrorBoundary FallbackComponent={PageErrorFallback}>
        <Suspense fallback={<LoadingIndicator />}>
          <Routes>
            <Route path="/" element={<Home />} />
            <Route path="/login" element={<LoginRoute />} />
            <Route path="/app/*" element={<AppRoute />} />
            <Route path="/submit" element={<SubmitRoute />} />
            <Route path="/submit/:slug" element={<SubmitRoute />} />
            <Route path="*" element={<h1>Not Found</h1>} />
          </Routes>
        </Suspense>
      </ErrorBoundary>
    </BrowserRouter>
  );
}
