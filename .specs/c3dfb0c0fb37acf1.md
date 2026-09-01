---
file: admin-ui/src/features/dashboard/DashboardPage.tsx
---

All-board dashboard page, displaying word database overall progress and resource gap overview.

## export

- **DashboardPage** — A page component with no parameters. Renders three stat cards at the top (total word count, ready word count, blocked word count); a ready-rate ring chart in the middle; four groups of bar charts side by side below, showing the gap counts for definition, example, image, and TTS resources respectively; and an event timeline at the bottom, listing recent system events in reverse chronological order. It can be directly mounted as a route page.

## constraint

- Accepts no props; all data is fetched from the backend API; don't attempt to pass through external status.
- Don't manually refresh dashboard data outside this component—the component internally manages polling and refresh logic on its own.
