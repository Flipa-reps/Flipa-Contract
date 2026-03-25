# Tossd Game History & Statistics Dashboard
Resolves Issue #87

## Overview
This design document details the layout, state definitions, and interactions for the Tossd Game History and Statistics Dashboard. The primary focus is establishing user trust through comprehensive accessibility and transparency regarding reserves, fees, and statistics.

## 1. Layout & Hierarchy
The dashboard is structurally designed for data-heavy inspection, organized into clear sections:
- **Global Transparency Header**: High-level overview of Reserve Visibility and Fee Transparency.
- **Contract Statistics Charts**: Visual trends over 24h, 7d, 30d periods.
- **Player History Table**: Detailed rows of player and game interactions.

> [!TIP]
> **Main Dashboard View**
> High-fidelity, detailed mockups establishing the neon-accented dark mode layout to emphasize visibility.
![Main Dashboard Mockup](./mockups/dashboard_main_mockup.png)

## 2. Interface States
To ensure a highly responsive implementation, all potential dashboard async states are defined below. 

### Loading State
When fetching large datasets from the blockchain/backend, skeletons are used to minimize layout shifts and give feedback of an ongoing process.
![Loading State](./mockups/dashboard_loading_state.png)

### Empty State
Visible for new users or fresh contracts before data is available. Focuses on a friendly call-to-action out of the dashboard.
![Empty State](./mockups/dashboard_empty_state.png)

### Error State
Triggers during data fetch failures. Features a clear, actionable error message with a "Retry" prompt to regain access smoothly.
![Error State](./mockups/dashboard_error_state.png)

## 3. Interaction Documentation

### Data Tables (Player History)
- **Sortable Columns**: Users can click column headers (e.g., Date, Wager, Outcome) to toggle ascending/descending order. Arrows dynamically indicate the current sort direction.
- **Row Highlights**: Hovering over a row subtly shifts the background contrast to maintain focus. Clicking a row expands inline cryptographic details (e.g., transaction hashes, proofs).
- **Pagination**: Implemented at the bottom right. Displays interactive elements like `< Previous | 1, 2, 3 ... 10 | Next >`.

### Charts (Contract Stats)
- **Time-frame Selectors**: Pill-shaped toggle buttons located top-right of the chart container (`24h`, `7d`, `30d`). Active states are highlighted with neon accent color.
- **Tooltip Behavior**: Hovering over data points on a line/bar chart pulls up a high z-index tooltip (dark background, light text) detailing exact values and timestamps, tracking the cursor along the X-axis exclusively.
- **Legends**: Clickable categorical legends below the chart toggle dataset visibility (e.g., hiding 'Total Fees' to isolate 'Reserve Balance').

## 4. Accessibility Guidelines
> [!IMPORTANT]
> Accessibility ensures trust is verifiable, not just decorative.

- **Color Contrast**: All text over backgrounds maintains a **minimum contrast ratio of 4.5:1** (WCAG AA compliant). Neon accents are used strictly for non-text graphical highlights or sufficiently thick headers.
- **Keyboard Navigation**: 
  - The `Tab` key sequentially focuses time-frame toggles, chart legends, table column headers, and pagination modules.
  - Interactive elements feature an undeniable outline (`:focus-visible` ring) to guarantee clear locational awareness for users relying safely on keyboard navigation.

## 5. Visual Design System (Color Palette)
To maintain the high-contrast dark mode fintech aesthetic representing the "Tossd" brand, the following hex codes are utilized:
- **Background (Primary)**: `#0F172A` (Deep Slate) - Defines the core dashboard backdrop.
- **Surface (Cards/Tables)**: `#1E293B` (Dark Blue-Grey) - Creates elevation for charts and data tables.
- **Primary Text**: `#F8FAFC` (Off-White) - Used for primary metrics, headings, and data points.
- **Secondary Text**: `#94A3B8` (Cool Grey) - Used for table headers, legends, and less emphasized text.
- **Brand/Neon Accent**: `#10B981` (Emerald Green) - Used for active states, time-frame toggles, and positive trends.
- **Error/Warning**: `#EF4444` (Bright Red) - Used for the Error State UI and negative metrics.

## 6. Responsive Layouts & Breakpoints
The dashboard is fully mobile-responsive, seamlessly adapting complex data views for smaller screens:
- **Desktop (≥ 1024px)**: Default layout. Charts and Header elements utilize a multi-column grid.
- **Tablet (768px - 1023px)**: Charts expand to 100% width, stacking vertically above the Player History Table.
- **Mobile (< 768px)**: 
  - All cards stack vertically in a single column (`flex-direction: column`).
  - The Player History table implements horizontal scrolling (`overflow-x: auto`) to prevent truncation of cryptographic hashes.
  - Interactive elements like time-frame selectors increase padding for touch targets.

## 7. Data Source Mapping
To ensure transparency and verifiable trust, the dashboard pulls metrics from the following deterministic data sources:
- **Reserve Visibility**: Fetched directly from the **Stellar Horizon API**, querying the authoritative game reserve account balance to prove cryptographic backing.
- **Contract Statistics & Fees**: Sourced via the **Soroban RPC**, aggregating total smart contract invocations, ledger occurrences, and accumulated protocol fees.
- **Player History Table**: Relies on a dedicated **Indexer Pipeline** (or custom backend) that ingests Tossd contract events to rapidly serve paginated, sortable game history and wager outcomes to the client.
