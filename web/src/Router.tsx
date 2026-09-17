import { BrowserRouter, Routes, Route } from "react-router";

/**
 * Placeholder shell. Real routes (`/login`, `/submit/:slug`, `/app/...`) land in
 * the web step of the build plan, once there is a Relay environment and an
 * auth story to route around.
 */
function Home() {
  return (
    <main className="flex min-h-screen flex-col items-center justify-center gap-2 text-center">
      <h1 className="text-2xl font-semibold text-ink-strong">microticket</h1>
      <p className="text-ink-muted">Coming soon.</p>
    </main>
  );
}

export default function Router() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Home />} />
      </Routes>
    </BrowserRouter>
  );
}
