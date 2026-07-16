# Create Study Wizard — Architecture Plan

## Context

Replace the monolithic form at `src/routes/_app/studies/new.tsx` with a professional multi-step wizard. The route stays at `/studies/new`; steps are internal UI state. No backend API exists yet — the form validates locally and logs the result.

**In scope:** 5-step wizard with progress indicator, per-step validation, animated transitions, review step with edit capability.

**Out of scope:** Backend submission, draft persistence (localStorage), file uploads, real-time collaboration.

---

## Constraints & Decisions

1. **Step state lives in URL search params (`?step=1`).** Enables browser back/forward navigation between steps. TanStack Router's `validateSearch` gives us type-safe access. The stepper and Next/Back buttons update the search param.

2. **Form data lives in a React `useReducer` at the route level.** Lightweight, no external dependencies, persists across step navigation within the session. The reducer holds the full `CreateStudyFormData` shape. Each step component receives the relevant slice + a dispatch/update callback.

3. **CSS-only transitions between steps.** The project already has `tw-animate-css`. Use a fade + subtle translateX for enter/exit. No additional animation library.

4. **No new form library.** The existing form uses plain controlled inputs. We keep that pattern. Validation is done via simple functions that return error maps — no Zod, no TanStack Form (those can be added when backend integration arrives).

5. **shadcn components to install:** `card` (for review step sections). Everything else (button, input, separator, badge) already exists.

6. **Stepper is a custom component.** No external stepper library. A horizontal step indicator with number badges, labels, connecting lines, and active/completed/upcoming states. Accessible via `aria-current="step"` and clickable only for completed + current steps.

7. **The route file remains the orchestrator.** It owns the form state, reads the step search param, renders the stepper + the active step component, and handles final "create" action.

---

## Structure

### File Layout

```
src/routes/_app/studies/new.tsx              # Route: orchestrator (state, stepper, step routing)
src/components/studies/create-wizard/
  index.ts                                   # Public API barrel
  types.ts                                   # CreateStudyFormData, StepConfig, validation types
  wizard-stepper.tsx                         # Horizontal step indicator
  step-study-info.tsx                        # Step 1: Study Information
  step-team-timeline.tsx                     # Step 2: Study Team & Timeline
  step-randomization-design.tsx             # Step 3: Randomization Design
  step-keys-strata.tsx                       # Step 4: Keys & Strata
  step-review.tsx                            # Step 5: Review & Confirm
  form-field.tsx                             # Shared FormField component (extracted from current)
  arm-editor.tsx                             # Reusable arm list editor (add/remove arms)
  key-editor.tsx                             # Reusable key list editor
  stratum-editor.tsx                         # Reusable stratum list editor
  validation.ts                              # Per-step validation functions
```

### Component Hierarchy

```
CreateStudyPage (route component)
  +-- WizardStepper (step={currentStep} steps={STEPS} onStepClick={...})
  +-- <div class="step-container"> (transition wrapper)
      +-- StepStudyInfo | StepTeamTimeline | StepRandomizationDesign | StepKeysStrata | StepReview
          +-- FormField (shared)
          +-- ArmEditor / KeyEditor / StratumEditor (step 3/4 only)
```

### Props Interfaces

```typescript
// types.ts

interface CreateStudyFormData {
  // Step 1: Study Information
  title: string;
  shortName: string;
  irbNumber: string;
  irbException: "none" | "waiver" | "exempt";
  protocolId: string;
  protocolLink: string;
  description: string;

  // Step 2: Team & Timeline
  piName: string;
  piEmail: string;
  coordinatorName: string;
  coordinatorEmail: string;
  statisticianName: string;
  statisticianEmail: string;
  sites: Array<{ id: string; name: string; enrollmentCap: string }>;
  startDate: string;
  endDate: string;
  targetEnrollment: string;

  // Step 3: Randomization Design
  strategy: RandomizationStrategy | "";
  blockSize: string;
  arms: Array<{ id: string; name: string; ratio: string }>;

  // Step 4: Keys & Strata
  keys: Array<{ id: string; name: string; type: KeyType | "" }>;
  strata: Array<{ id: string; name: string; type: "fhir-path" | "custom" | ""; values: string[] }>;
}

interface StepProps {
  data: CreateStudyFormData;
  errors: Record<string, string>;
  onChange: (field: string, value: unknown) => void;
}

interface StepConfig {
  id: number;
  label: string;
  shortLabel: string;
  validate: (data: CreateStudyFormData) => Record<string, string>;
}
```

### Dependency Direction

```
route (new.tsx)
  --> create-wizard/types.ts
  --> create-wizard/wizard-stepper.tsx
  --> create-wizard/step-*.tsx
  --> create-wizard/validation.ts

step-*.tsx
  --> create-wizard/types.ts
  --> create-wizard/form-field.tsx
  --> create-wizard/arm-editor.tsx (step 3)
  --> create-wizard/key-editor.tsx (step 4)
  --> create-wizard/stratum-editor.tsx (step 4)
  --> @/components/ui/* (button, input, separator, card, badge)
  --> #/lib/types/* (domain enums for select options)

wizard-stepper.tsx
  --> create-wizard/types.ts (StepConfig)
  --> @/components/ui/button (for clickable steps)

validation.ts
  --> create-wizard/types.ts (CreateStudyFormData)
```

---

## State Management

### URL Search Param (step navigation)

```typescript
// In new.tsx route definition
import { z } from "zod";  // Note: only for search param validation, not form

const wizardSearchSchema = z.object({
  step: z.number().int().min(1).max(5).default(1),
});

export const Route = createFileRoute("/_app/studies/new")({
  validateSearch: wizardSearchSchema,
  component: CreateStudyPage,
});
```

**Decision note:** If adding Zod solely for search params feels heavy (it's not currently a dependency), fall back to manual parsing: `const step = Math.min(5, Math.max(1, Number(search.step) || 1))`. The builder should check if Zod is already reachable via another dependency — if not, manual parsing is fine.

### Form State (useReducer)

```typescript
type FormAction =
  | { type: "UPDATE_FIELD"; field: string; value: unknown }
  | { type: "UPDATE_FIELDS"; fields: Partial<CreateStudyFormData> }
  | { type: "ADD_ARM" }
  | { type: "REMOVE_ARM"; id: string }
  | { type: "ADD_KEY" }
  | { type: "REMOVE_KEY"; id: string }
  | { type: "ADD_STRATUM" }
  | { type: "REMOVE_STRATUM"; id: string }
  | { type: "ADD_SITE" }
  | { type: "REMOVE_SITE"; id: string }
  | { type: "RESET" };
```

The reducer handles all mutations. Step components call `dispatch` for changes. The route component owns the `useReducer` and passes `data` + `dispatch` down.

### Navigation Logic

```
handleNext():
  1. Run validate(currentStep, formData)
  2. If errors → set errors state, do not navigate
  3. If valid → navigate({ search: { step: currentStep + 1 } })
  4. Mark currentStep as "completed" in visited set

handleBack():
  1. navigate({ search: { step: currentStep - 1 } })
  (no validation needed)

handleStepClick(targetStep):
  1. If targetStep <= maxCompletedStep + 1 → navigate
  2. Otherwise → ignore (can't skip ahead)
```

---

## Build Sequence

### Increment 1: Foundation [TDD]

**Files:** `types.ts`, `validation.ts`, `form-field.tsx`

- Define `CreateStudyFormData` with initial defaults
- Define `StepConfig` array (5 steps with labels)
- Implement validation functions for each step (pure functions, easily testable)
- Extract `FormField` from existing code (already exists inline in `new.tsx`)

**Verifies:** Unit tests for validation functions pass. FormField renders with label + required indicator.

### Increment 2: Stepper Component [TDD]

**Files:** `wizard-stepper.tsx`

- Horizontal layout: number badges + labels + connecting lines
- States: completed (checkmark), current (primary color, bold), upcoming (muted)
- Clickable: completed and current steps fire `onStepClick`; upcoming steps are inert
- Accessible: `aria-current="step"` on current, `role="list"` on container

**Verifies:** Stepper renders correct states. Click fires callback for valid targets. Does not fire for future steps.

### Increment 3: Step Components [parallel]

**Files:** `step-study-info.tsx`, `step-team-timeline.tsx`, `step-randomization-design.tsx`, `step-keys-strata.tsx`, `step-review.tsx`

These can be built in parallel since they share only `types.ts` and `form-field.tsx`.

- Each step renders its fields using `FormField`
- Receives `data`, `errors`, `onChange` via props
- Shows inline validation errors per field
- Step 3 includes `arm-editor.tsx` (dynamic list: add/remove arms)
- Step 4 includes `key-editor.tsx` and `stratum-editor.tsx`
- Step 5 (review) renders a read-only summary with "Edit" buttons per section that jump to the relevant step

**Verifies:** Each step renders its fields. Errors display correctly. Dynamic lists add/remove items.

### Increment 4: Route Orchestration [parallel with Increment 3]

**Files:** `new.tsx` (rewrite), `index.ts`

- Read `step` from search params
- Initialize `useReducer` with form defaults
- Render: back link, heading, WizardStepper, active step, navigation buttons
- Wire handleNext/handleBack/handleStepClick
- Add CSS transition classes on step container
- Final step: "Create Study" button → console.log(formData) + show success state

**Verifies:** Step navigation works via buttons and stepper clicks. Validation blocks Next on required fields. Data persists across steps.

### Increment 5: Polish & Install [parallel]

- Install shadcn `card` component: `npx shadcn@latest add card`
- CSS transitions: fade + translateX on step change (use `tw-animate-css` classes or custom keyframes in the component)
- Focus management: move focus to step heading on navigation
- Responsive: stack stepper vertically on mobile (`sm:` breakpoint)
- Success state after creation: confirmation message with link back to studies list

**Verifies:** Transitions animate smoothly. Focus moves correctly. Mobile layout works.

---

## Testing Strategy

### Unit Tests (Vitest)

| File | What to Test |
|------|-------------|
| `validation.test.ts` | Each step's validator: required fields produce errors, optional fields don't, edge cases (empty arms array, invalid URLs) |
| `wizard-stepper.test.tsx` | Renders correct number of steps. Current step highlighted. Click on completed step fires callback. Click on future step does nothing. Accessibility attributes present. |
| `form-field.test.tsx` | Renders label, required indicator, children. Shows error when provided. |
| `arm-editor.test.tsx` | Add arm creates new entry. Remove arm removes correct entry. Minimum 2 arms enforced. |

### Integration Tests (Vitest + RTL)

| File | What to Test |
|------|-------------|
| `new.test.tsx` | Full wizard flow: fill step 1 → next → fill step 2 → ... → review → create. Data persists across back/forward. Validation blocks next. Step clicking respects rules. |

### Test Patterns for Multi-Step Forms

```typescript
// Navigate to a step by filling previous steps
async function fillAndAdvance(user: UserEvent, step: number) {
  // Fill minimum required fields for each step up to `step`
  // Click Next after each
}

// Assert step is active
function expectStep(step: number) {
  expect(screen.getByRole("heading", { level: 2 })).toHaveTextContent(STEP_HEADINGS[step - 1]);
}
```

### Edge Cases to Cover

- Back navigation preserves all previously entered data
- Required field validation: empty title, empty IRB number, zero arms
- Step jumping: can click step 2 after completing step 1, cannot click step 4 from step 1
- Dynamic lists: adding then removing all arms, then trying to advance
- Browser back button (search param change) updates displayed step
- Review step displays all entered data accurately
- "Edit" button on review step navigates to correct step

---

## Docs

- No public API exposed outside the route — this is a page-level feature
- Internal: TSDoc on `StepConfig`, `CreateStudyFormData`, and validation functions (they're the contract between orchestrator and steps)
- The review step effectively documents all fields for the user

---

## Observability

- `console.log` the final `CreateStudyFormData` on "Create" (placeholder for future API call)
- No tracing, no error reporting — this is a local-only form with no backend
- When backend integration arrives: add error boundaries around the submit action

---

## Animation Approach

Use CSS classes toggled on step change. The step container gets a transition class based on navigation direction:

```css
/* Forward: slide in from right */
.step-enter-forward {
  animation: slideInRight 200ms ease-out;
}

/* Backward: slide in from left */
.step-enter-backward {
  animation: slideInLeft 200ms ease-out;
}

@keyframes slideInRight {
  from { opacity: 0; transform: translateX(16px); }
  to { opacity: 1; transform: translateX(0); }
}

@keyframes slideInLeft {
  from { opacity: 0; transform: translateX(-16px); }
  to { opacity: 1; transform: translateX(0); }
}
```

Keep a `direction` state (`"forward" | "backward"`) updated on navigation. Apply the class to a wrapper `<div>` with a `key={currentStep}` so React remounts on step change, triggering the animation.

---

## Stepper Visual Design

```
  (1)────────(2)────────(3)────────(4)────────(5)
  Study      Team &     Randomize   Keys &    Review
  Info       Timeline   Design      Strata
```

- **Completed:** Filled primary-color circle with checkmark icon, primary-color connecting line
- **Current:** Filled primary-color circle with step number, bold label, pulsing ring (subtle)
- **Upcoming:** Muted border circle with step number, muted label, muted connecting line
- **Mobile (< sm):** Vertical layout or collapsed to just numbers with current label shown below

---

## For the Builder

1. **Zod dependency check.** Zod is NOT currently in `package.json`. The search param validation needs either: (a) install Zod and use `validateSearch`, or (b) manual parsing in a `beforeLoad` or `loaderDeps`. Decision: use manual parsing to avoid adding a dependency solely for one search param. If the team decides to add Zod later for form validation, switch then.

2. **Transition key approach.** Using `key={currentStep}` forces a remount, which resets internal input state. Since all state is lifted to the reducer, this is fine — inputs are controlled. But verify that focus management still works (focus the step heading after mount via `useEffect`).

3. **Step 5 "Edit" buttons.** These navigate via search params (`?step=N`). The visited/completed tracking must not regress — clicking "Edit" from review and going back to step 2 should not reset steps 3-5 as "incomplete." Track `maxReachedStep` separately from `currentStep`.

4. **Sites list in Step 2.** The domain type `StudySite` has an `enrollmentCap`. Decide whether this is a number input or optional. For now: optional number input, default empty string → parsed to `undefined` on creation.

5. **Arm ratios.** Must be positive integers. The minimum arm count is 2. Enforce this in validation and disable the "Remove" button when at minimum.

---

## For the Reviewers

### Architecture
- Single route file as orchestrator: verify it doesn't bloat past ~150 lines. Step logic should live in step components.
- `useReducer` vs `useState`: reducer is correct here because form actions are complex (add/remove from arrays, update nested fields). Verify the reducer doesn't become a god function — consider splitting into sub-reducers per step if it exceeds ~80 lines.
- URL search param for step: verify browser back/forward works correctly in integration tests.

### Style
- Tabs for indentation, double quotes, named function declarations, `#/` imports for app code, `@/` for shadcn.
- No `React.FC`, no default exports, no `useMemo`/`useCallback`.
- File names in kebab-case (matches existing `src/components/` convention).

### Frontend (Accessibility)
- **Focus management:** On step change, focus moves to the step's heading (`<h2>`) or first input. Verify with keyboard-only navigation.
- **Stepper accessibility:** `role="list"` + `role="listitem"`, `aria-current="step"` on active step, descriptive labels.
- **Form labels:** Every input has an associated `<label>`. Required fields indicated visually AND via `aria-required`.
- **Error announcements:** Validation errors should be associated with inputs via `aria-describedby` and optionally announced via `aria-live="polite"` region.
- **Keyboard navigation:** Tab through inputs, Enter/Space on buttons, stepper steps focusable and activatable via keyboard.
- **Responsive:** Wizard usable at 320px viewport width. Stepper collapses gracefully.

### Testing
- Validation functions are pure — test exhaustively without rendering.
- Integration test covers the full flow (fill all 5 steps, verify review shows correct data, verify "create" action fires).
- Step navigation edge cases: verify you cannot skip steps, verify back preserves data.

---

## Not in This Plan

- LocalStorage draft persistence (save partially filled wizard) — useful but separate concern
- Backend API submission — blocked on backend endpoint availability
- File attachments (protocol documents) — future enhancement
- Multi-user collaboration on study creation — out of scope
- Undo/redo within the form — overkill for this use case
