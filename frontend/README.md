# Frontend

Run `npm install` and `npm run dev`, then open http://localhost:3001.
Run the Rust backend separately on port 3000. The frontend requests `/api/*` and
uses `/images/*` on its own origin. Vite proxies both paths unchanged to the backend
in development and preview, so no browser CORS setup is needed. Set `BACKEND_URL`
when starting Vite to override the proxy target (default `http://127.0.0.1:3000`).
SSR chart loaders resolve API paths against the incoming request origin.

In production, configure your reverse proxy to send `/api/*` and `/images/*` to
the Rust service and all other paths to the frontend, preserving request paths.
Vite proxy settings do not configure the production server. The frontend needs
no public backend URL; the same domain must also be reachable by the SSR server.

- `/`: masked date/time fields and Headless UI place autocomplete.
- `/chart/{latitude}/{longitude}/{time}`: loads and pretty-prints the complete backend JSON.
- `src/components`: reusable, unstyled field components with CSS classes and Headless UI data attributes.
- `src/lib/birth-time.ts`: strict calendar/time validation and IANA timezone conversion.

Times that are nonexistent or ambiguous during daylight-saving changes are rejected
with an inline error; choosing a DST occurrence is not implemented yet.

Validation: `npx tsc --noEmit`, `npm run build`, `npm run check`.
Styling uses plain CSS; styled-components is also available. No Tailwind.
