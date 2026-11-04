-- Standalone administration; no dependency on an external identity database.
CREATE TABLE admin_sessions (
    token_hash TEXT PRIMARY KEY,
    username TEXT NOT NULL,
    credential_fingerprint TEXT NOT NULL,
    csrf_token TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX admin_sessions_expiry ON admin_sessions(expires_at);
CREATE TABLE admin_login_limit (
    id BOOLEAN PRIMARY KEY DEFAULT true CHECK (id),
    attempts INTEGER NOT NULL DEFAULT 0,
    window_start TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
INSERT INTO admin_login_limit (id) VALUES (true);
CREATE TABLE admin_import_jobs (
    id UUID PRIMARY KEY,
    state TEXT NOT NULL CHECK (state IN ('uploading','inspecting','ready','queued','validating','publishing','succeeded','failed','cancelled','interrupted')),
    bytes BIGINT NOT NULL DEFAULT 0,
    sha256 TEXT,
    inspection JSONB,
    result JSONB,
    error TEXT,
    cancel_requested BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX admin_import_jobs_updated ON admin_import_jobs(updated_at);
