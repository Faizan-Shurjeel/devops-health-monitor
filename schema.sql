-- Database schema for DevOps Health Monitor

CREATE TABLE IF NOT EXISTS targets (
    id SERIAL PRIMARY KEY,
    url TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS health_checks (
    id SERIAL PRIMARY KEY,
    target_id INTEGER NOT NULL REFERENCES targets(id) ON DELETE CASCADE,
    checked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    status_code INTEGER,
    response_time_ms INTEGER
);

-- Helpful index for querying recent health checks per target
CREATE INDEX IF NOT EXISTS idx_health_checks_target_checked_at
ON health_checks (target_id, checked_at DESC);
