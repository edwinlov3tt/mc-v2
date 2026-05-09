# Phase 6B Design Brief — Web UI

> **Status:** Design requirements (pre-ADR)  
> **Date:** 2026-05-08  
> **Reference prototype:** `/Users/edwinlovettiii/projects/mosaic2/` (bolt.new grid prototype)  
> **Target:** Phase 6B — the web UI that renders Phase 6A's CLI capability layer visually

---

## 1. Design Direction

The bolt.new prototype is functional but feels like an enterprise spreadsheet tool. The target UX is **Airtable meets Notion** — warm, inviting, approachable, with personality. Not a tool that makes you feel like you need training; a tool that makes you want to explore.

### What to keep from the prototype
- Three-panel layout (sidebar / grid / inspector) — proven pattern
- Formula bar concept — shows what's behind a cell
- Grid with arrow-key navigation + inline editing
- Version comparison mode (diff columns)
- Scenario/version pills in the top bar
- Saved views in the sidebar

### What to change
- **Warmer palette.** The prototype's `ink-900` through `ink-50` reads cold/corporate. Shift toward warmer neutrals (Airtable's warm grays, slight cream undertones)
- **More whitespace and breathing room.** The prototype packs information densely. Add more padding, larger row heights, more gap between sections
- **Rounded, softer elements.** Larger border-radius on cards, pills, buttons. Less sharp edges
- **Color as meaning, not decoration.** Input cells, derived cells, consolidated cells, and edited cells should have distinct, gentle background tints — not just text color changes
- **Typography hierarchy.** The prototype uses one font weight throughout. Add more weight variation — light labels, medium body, semibold headers
- **Personality touches.** Subtle hover animations, smooth transitions, a pleasant empty state, friendly onboarding

### Design references
- **Airtable:** warmth, approachability, color-coded fields, friendly empty states
- **Notion:** clean typography, generous whitespace, calm but capable
- **Linear:** modern dark mode option, keyboard-first, smooth animations
- **Figma:** collaborative feel, contextual panels, clean tool UI

---

## 2. Phased Approach

### Phase 6B.0: Design System + HTML Prototypes (~1-2 sessions)

Ship a standalone HTML/CSS design system BEFORE building React components. This lets you iterate on the visual language cheaply — colors, typography, spacing, component shapes — without touching the real app.

**Deliverables:**

1. **Design tokens** (CSS custom properties):
   ```css
   :root {
     /* Warm neutrals */
     --surface-0: #ffffff;
     --surface-1: #fafaf8;      /* warm sunken */
     --surface-2: #f5f4f1;      /* warm muted */
     --border: #e8e6e1;         /* warm border */
     --border-strong: #d5d2cc;
     
     /* Text */
     --text-primary: #1a1816;
     --text-secondary: #6b6560;
     --text-tertiary: #9c9691;
     
     /* Accents */
     --accent: #6366f1;         /* indigo — primary action */
     --accent-soft: #eef2ff;
     --success: #16a34a;
     --warning: #d97706;
     --danger: #dc2626;
     
     /* Cell states */
     --cell-input: transparent;
     --cell-derived: #f0f7ff;   /* soft blue tint */
     --cell-consolidated: #f5f0ff; /* soft purple tint */
     --cell-edited: #fefce8;    /* soft yellow tint */
     --cell-locked: #f9fafb;    /* gray tint */
     
     /* Spacing scale */
     --space-1: 4px;
     --space-2: 8px;
     --space-3: 12px;
     --space-4: 16px;
     --space-6: 24px;
     --space-8: 32px;
     
     /* Radius */
     --radius-sm: 6px;
     --radius-md: 10px;
     --radius-lg: 16px;
     --radius-full: 9999px;
     
     /* Typography */
     --font-sans: 'Inter', system-ui, sans-serif;
     --font-mono: 'JetBrains Mono', monospace;
   }
   ```

2. **Component prototypes** (static HTML, no JS):
   - Grid row (input / derived / consolidated / group header)
   - Cell states (clean / edited / selected / locked)
   - Top bar with scenario/version pills
   - Sidebar with saved views
   - Inspector panel (cell detail / trace / versions)
   - Formula bar
   - Empty state ("No model loaded")
   - Toast notifications
   - Dimension slice pills
   - Dropdown/popover

3. **Dark mode variant** — same components, dark tokens

Ship as `demo/design-system/index.html` — a single page showing all components in all states. This is the visual reference the implementer builds against.

### Phase 6B.1: Shell + Navigation

- App shell (three-panel layout with resize handles)
- Top bar: model selector, scenario/version pills, dimension slice pins
- Sidebar: workspace file tree, saved views
- Route structure: `/` (grid), `/trace` (trace view), `/analysis` (narratives)
- Connect to `mc model query` via API for real data

### Phase 6B.2: Grid + Editing

- Spreadsheet grid with virtual scrolling (for large models)
- Row hierarchy (expand/collapse groups)
- Cell types (input / derived / consolidated) with visual distinction
- Inline editing with writeback via `mc model write`
- Arrow key navigation, Tab, Enter, Escape
- Formula bar showing the selected cell's rule/value
- Dirty cell tracking + "Calculate" button

### Phase 6B.3: Trace + Inspector

- Cell inspector panel (right sidebar)
- Trace view: `mc model trace` rendered as a dependency tree
- Version comparison: side-by-side columns with diff highlighting
- Snapshot/rollback via `mc model` snapshot commands

### Phase 6B.4: Narratives + Analysis

- Embed the narrative engine output in a dedicated tab/panel
- Upload flow (CSV/PPTX) integrated into the workspace
- Narrative sidebar showing fired templates with severity colors
- Explanation chain visualization (which explanation won, which were rejected)

---

## 3. Technology Stack

- **React 18 + TypeScript** (same as demo frontend and bolt.new prototype)
- **Vite** (same build tooling)
- **Tailwind CSS** with custom design tokens (extending the token system above)
- **No component library** — hand-built components for full control over the visual language
- **lucide-react** for icons (already used in the prototype)
- **Virtual scrolling** via a lightweight virtualizer (e.g., `@tanstack/react-virtual`) for grids with 1000+ rows

### What NOT to use
- No MUI/Chakra/Radix — too opinionated, fights the custom design language
- No Redux — React state + context is sufficient for this app size
- No Next.js/Remix — this is a SPA that talks to the Rust backend, not an SSR app
- No Electron — it's a web app served by `mc start`, not a desktop app

---

## 4. API Surface

The UI consumes the Phase 6A CLI capability layer via HTTP API. The demo server (`mc-demo-server`) already serves some of these; Phase 6B extends it or replaces it with a purpose-built API server.

| UI Action | API Call | Phase 6A Equivalent |
|---|---|---|
| Load grid data | `GET /api/query?model=...&scenario=...&version=...` | `mc model query` |
| Edit a cell | `POST /api/write` | `mc model write` |
| Trace a cell | `GET /api/trace?cell=...` | `mc model trace` |
| What-if | `POST /api/whatif` | `mc model whatif` |
| Sweep parameters | `POST /api/sweep` | `mc model sweep` |
| Compare versions | `GET /api/diff?v1=...&v2=...` | `mc model diff` |
| Run narratives | `POST /api/narrate` | `mc model narrate` |
| List models | `GET /api/models` | File system scan |
| Upload data | `POST /api/upload` | Existing upload endpoint |

---

## 5. What Makes This Different from the Prototype

The prototype has static data in `src/data/cubes.ts`. Phase 6B uses real data from real models via the CLI capability layer. The grid shows actual `mc model query` output; edits call `mc model write`; traces call `mc model trace`. The UI is a rendering layer over the existing capability — it doesn't add capability, it presents it.

This means:
- Every number in the grid is the same number `mc model query` would print
- Every trace tree is the same tree `mc model trace` would print
- Every narrative is the same narrative `mc model narrate` would produce
- The UI is the last 10% of effort that makes the first 90% accessible

---

*End of design brief. Phase 6B.0 (design system + prototypes) ships first as static HTML. Iterate on the visual language before building components. The prototype at mosaic2/ is the functional reference; the design system establishes the emotional reference.*
