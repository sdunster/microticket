# microticket web UI

> **First-time setup is in [../DEVELOPMENT.md](../DEVELOPMENT.md)** — running the full stack with
> `make dev` / `make dev-local`. This file covers web-specific commands.

Requires Node.js >= 22.

- Install dependencies: `npm i`
- Run the Relay compiler: `npm run relay` (or `npm run relay -- --watch`) — regenerates
  `src/**/__generated__`, which is gitignored and rebuilt automatically before `build`,
  `typecheck` and `test`/`test:unit` (see the `pre*` scripts in `package.json`)
- Run the dev server with hot module reloading: `npm run dev` (or `npm run start`)
- Build production assets: `npm run build`
- Run all web unit tests: `npm run test:unit`
- Run a single unit test file: `npm run test:unit -- src/lib/tw.test.ts`
- Typecheck only: `npm run typecheck`
- Lint: `npm run lint`
- Format: `npm run format`
