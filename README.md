# DevOps Health Monitor

A high-performance DevOps Health Check Monitor built with Rust (Axum), SQLx, Postgres, and deployed on Shuttle.rs. A simple SPA frontend (vanilla JS + Chart.js) can be deployed on Vercel.

## Features

- Periodic background worker (60s) to check target URLs via HTTP
- Stores status code and response time in Postgres (Supabase-compatible)
- Axum JSON API:
  - `GET /api/targets`
  - `GET /api/status/:target_id`
- SPA dashboard with Chart.js visualization

## Database Schema

See `schema.sql` and the inline schema created on startup.

## Local Development

1. Install Rust and Shuttle CLI.
2. Start Postgres locally or use Supabase. Create the schema:

```sql
-- Run these in your Postgres instance
\i schema.sql
```

3. Copy secrets example:

```bash
# PowerShell
Copy-Item Secrets.toml.example Secrets.toml
```

Then set `DATABASE_URL` for local if not using Shuttle DB, and optionally `SEED_URLS`.

4. Run locally with Shuttle:

```bash
cargo shuttle run
```

5. Frontend
   Deploy `frontend/` to Vercel (static). Configure the API base URL via the `?api=` query param on the deployed site or save it once via localStorage. Example:

```
https://your-frontend.vercel.app/?api=https://your-shuttle-app.shuttleapp.rs
```

## Deployment

- Backend: `cargo shuttle deploy`
- Frontend: deploy `frontend/` as a static site on Vercel.

## Notes

- The background worker runs in-process and at-least-once per 60 seconds. If multiple instances are scaled, consider leader election or a job queue to avoid duplicate checks.
