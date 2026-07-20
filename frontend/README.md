# POCR Frontend

Web interface for **POCR** — the UCSF Point of Care Randomization Engine. This is a Vite/React single-page application for configuring randomization studies, allocating subjects at the point of care, and monitoring enrollment in real time. It talks to a Rust backend service over HTTP.

## Prerequisites

- Node.js 20+
- npm 10+

## Getting started

```bash
npm install
npm run dev
```

The dev server runs on [http://localhost:3000](http://localhost:3000).

## Scripts

| Command | Description |
| --- | --- |
| `npm run dev` | Start the dev server on port 3000. |
| `npm run build` | Production build to `dist/`. |
| `npm run preview` | Serve the production build locally. |
| `npm run test` | Run the test suite (Vitest). |
| `npm run check` | Lint and format with Biome (auto-fix). |
| `npm run lint` | Lint only (auto-fix). |
| `npm run format` | Format only (auto-fix). |

Run a single test file with `npx vitest run src/path/to/file.test.tsx`.

## Tech stack

- **React 19** + **Vite** — SPA runtime and build tooling.
- **TanStack Router** — file-based routing (`src/routes/`); the route tree in `src/routeTree.gen.ts` is generated, do not edit it.
- **TanStack Query** — server state and data fetching.
- **Tailwind CSS v4** + **shadcn/ui** (Base UI primitives, CVA variants) — styling and components.
- **Biome** — linting and formatting (tabs, double quotes).
- **Vitest** + React Testing Library — testing (jsdom).

## Project layout

```
src/
  routes/        File-based routes (studies, strata, context, reports, users, system)
  api/           Backend HTTP client and query provider
  components/    UI components and study create wizard
  lib/           Types, constants, and mocks
```

Path aliases `#/*` and `@/*` both resolve to `src/`. Prefer `#/` for application imports.

## Deployment

A multi-stage Docker build compiles the app with `vite build` and serves the static output from `nginx:alpine` on port 8080. In production the app sits behind an nginx reverse proxy with Vouch for OAuth/OIDC authentication. See the repository root for full-stack orchestration (engine, Redis, nginx, Vouch).
