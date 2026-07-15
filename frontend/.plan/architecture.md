# POCR UI Skeleton — Architecture Plan (v2)

## Context

Build the UI skeleton for the POCR (Point-of-Care Randomization) frontend. Vite + React 19 + TanStack Router (file-based) + Tailwind v4 + shadcn/ui (base-nova) SPA.

Porting concepts from the NextJS POC but rethinking the design:

- **Layout:** Top nav (branding, user) + collapsible sidebar (section navigation)
- **Study lifecycle:** Draft -> Testing -> Active -> Completed -> Archived
- **Operational status (when Active):** Running | Paused | Suspended
- **Navigation sections:** Studies, Strata, Context, Reports, Users
- No backend API calls — hardcoded mock data and empty states only

---

## Layout

```
┌──────────────────────────────────────────────────┐
│  [Logo] POCR                    [User] [Settings]│  ← Top nav
├────────────┬─────────────────────────────────────┤
│ Studies    │                                     │
│ Strata     │         Page Content                │  ← Sidebar + content
│ Context    │                                     │
│ Reports    │                                     │
│ Users      │                                     │
└────────────┴─────────────────────────────────────┘
```

---

## Route Structure

```
src/routes/
├── __root.tsx                          # Root: Outlet + DevTools (exists)
├── _app.tsx                            # Layout: top nav + sidebar + Outlet
├── _app/
│   ├── index.tsx                       # / → redirect to /studies
│   ├── studies/
│   │   ├── index.tsx                   # Dashboard: stats cards + study table
│   │   ├── new.tsx                     # Create study (multi-section form)
│   │   ├── $studyId.tsx                # Study detail layout (tabs)
│   │   └── $studyId/
│   │       ├── index.tsx               # Overview tab
│   │       ├── config.tsx              # Randomization config tab
│   │       └── monitor.tsx             # Monitoring/enrollment tab
│   ├── strata/
│   │   └── index.tsx                   # Strata management (placeholder)
│   ├── context/
│   │   └── index.tsx                   # Context config (placeholder)
│   ├── reports/
│   │   └── index.tsx                   # Reports (placeholder)
│   └── users/
│       └── index.tsx                   # User management (placeholder)
```

---

## Study State Machine

### Lifecycle (sequential)

```
Draft → Testing → Active → Completed → Archived
```

| State | Description |
|-------|-------------|
| Draft | Being designed, config incomplete |
| Testing | Config locked, dry-run validation |
| Active | Live, accepting real randomizations |
| Completed | Enrollment target met, no new randomizations |
| Archived | Historical record, read-only |

### Operational Status (parallel, Active only)

```
Running ↔ Paused ↔ Suspended
```

| Status | Description |
|--------|-------------|
| Running | Accepting randomization requests |
| Paused | Temporarily halted (protocol amendment, site issue) |
| Suspended | Safety hold (DSMB, adverse event), requires review to resume |

---

## Domain Types

### src/lib/types/study.ts

```typescript
export enum StudyStatus {
  Draft = "draft",
  Testing = "testing",
  Active = "active",
  Completed = "completed",
  Archived = "archived",
}

export enum OperationalStatus {
  Running = "running",
  Paused = "paused",
  Suspended = "suspended",
}

export interface StudyContact {
  name: string;
  email: string;
  role: "pi" | "coordinator" | "statistician";
}

export interface StudySite {
  id: string;
  name: string;
  enrollmentCap?: number;
}

export interface Study {
  id: string;
  title: string;
  shortName: string;
  status: StudyStatus;
  operationalStatus?: OperationalStatus;
  irbNumber: string;
  irbException: "none" | "waiver" | "exempt";
  protocolId?: string;
  protocolLink?: string;
  description: string;
  contacts: StudyContact[];
  sites: StudySite[];
  targetEnrollment: number;
  currentEnrollment: number;
  startDate?: string;
  endDate?: string;
  randomizationConfig: RandomizationConfig;
  createdAt: string;
  updatedAt: string;
}
```

### src/lib/types/randomization.ts

```typescript
export enum RandomizationStrategy {
  Simple = "simple",
  Block = "block",
  StratifiedBlock = "stratified-block",
  Minimization = "minimization",
}

export enum KeyType {
  PatientId = "patient-id",
  EncounterId = "encounter-id",
  ProviderId = "provider-id",
  ClusterId = "cluster-id",
  FhirPath = "fhir-path",
}

export interface Arm {
  id: string;
  name: string;
  ratio: number;
}

export interface Key {
  id: string;
  name: string;
  type: KeyType;
}

export interface Stratum {
  id: string;
  name: string;
  type: "fhir-path" | "custom";
  values?: string[];
}

export interface RandomizationConfig {
  strategy: RandomizationStrategy;
  arms: Arm[];
  keys: Key[];
  strata: Stratum[];
  blockSize?: number;
}
```

---

## File Ownership

### coder-shell (Slice 1 + 2)

- `src/lib/types/study.ts`
- `src/lib/types/randomization.ts`
- `src/lib/mocks/studies.ts`
- `src/routes/_app.tsx`
- `src/components/shell/app-sidebar.tsx`
- `src/components/shell/app-header.tsx`
- Install shadcn components (sidebar, badge, avatar, separator, tooltip)
- Remove old `src/routes/index.tsx`

### coder-pages (Slice 3 + 4)

- `src/routes/_app/index.tsx`
- `src/routes/_app/studies/index.tsx`
- `src/routes/_app/studies/new.tsx`
- `src/routes/_app/studies/$studyId.tsx`
- `src/routes/_app/studies/$studyId/index.tsx`
- `src/routes/_app/studies/$studyId/config.tsx`
- `src/routes/_app/studies/$studyId/monitor.tsx`
- `src/routes/_app/strata/index.tsx`
- `src/routes/_app/context/index.tsx`
- `src/routes/_app/reports/index.tsx`
- `src/routes/_app/users/index.tsx`

### test-coder-shell

- `src/test/setup.ts` (test infrastructure)
- `src/components/shell/app-sidebar.test.tsx`
- `src/components/shell/app-header.test.tsx`

### test-coder-pages

- `src/routes/_app/studies/index.test.tsx`
- `src/routes/_app/studies/new.test.tsx`
- `src/routes/_app/studies/$studyId.test.tsx`

---

## Code Patterns

### Route File

```tsx
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/studies/")({
  component: StudiesPage,
});

function StudiesPage() {
  return (
    <div className="p-6">
      {/* content */}
    </div>
  );
}
```

### Layout Route

```tsx
import { createFileRoute, Outlet } from "@tanstack/react-router";

export const Route = createFileRoute("/_app")({
  component: AppLayout,
});

function AppLayout() {
  return (
    <div className="flex h-screen flex-col">
      <AppHeader />
      <div className="flex flex-1 overflow-hidden">
        <AppSidebar />
        <main className="flex-1 overflow-auto">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
```

### Active Link (sidebar)

```tsx
<Link
  to="/studies"
  activeProps={{ className: "bg-accent text-accent-foreground" }}
  inactiveProps={{ className: "text-muted-foreground" }}
  activeOptions={{ exact: false }}
>
  Studies
</Link>
```

### Active Link (tabs, exact match)

```tsx
<Link
  to="/studies/$studyId"
  params={{ studyId }}
  activeProps={{ className: "border-b-2 border-primary" }}
  activeOptions={{ exact: true }}
>
  Overview
</Link>
```

### Redirect

```tsx
import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/")({
  beforeLoad: () => {
    throw redirect({ to: "/studies" });
  },
});
```

---

## Visual Direction (from Figma mockups)

- Blue primary accent color (update CSS custom properties)
- Clean, spacious layout with generous padding
- Stats cards row on dashboard (Active, Total Randomized, In Setup, Completed)
- Data table with status badges (colored dots + text)
- Forms use 2-column grid for short fields, full-width for textareas
- Prominent "Create New Study" CTA button
- Professional, clinical/medical application feel

---

## For the Reviewers

### Architecture
- Pathless `_app.tsx` layout route wraps all authenticated content
- `$studyId.tsx` + `$studyId/` for tabbed detail layout
- URL-driven tabs (each tab = real route)
- Types in `src/lib/types/`, mocks in `src/lib/mocks/`

### Style
- Tabs for indentation, double quotes (Biome)
- `#/` imports for app code
- Named function declarations for components
- No React.FC, no default exports for route components
- Tailwind v4 class names

### Frontend
- Accessible: headings, aria labels, keyboard nav
- Active link states on sidebar and tabs
- Empty states with helpful CTAs
- Responsive sidebar (collapsible)
