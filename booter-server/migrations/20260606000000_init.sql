-- Create tables that must exist
CREATE TABLE IF NOT EXISTS users (
    email TEXT PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS system_config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS companions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    scripts TEXT NOT NULL DEFAULT '{}',
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Ensure admins table exists so the migration script doesn't fail on a fresh DB
CREATE TABLE IF NOT EXISTS admins (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    totp_secret TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Migrate existing totp_secret if it's there
INSERT OR IGNORE INTO system_config (key, value)
SELECT 'admin_totp_secret', totp_secret
FROM admins
WHERE id = 1 AND totp_secret IS NOT NULL;

-- Now drop the admins table
DROP TABLE admins;

-- Create the new sessions table
CREATE TABLE IF NOT EXISTS sessions (
    token TEXT PRIMARY KEY,
    role TEXT NOT NULL,
    email TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    last_used_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Create the user_logs table
CREATE TABLE IF NOT EXISTS user_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    email TEXT NOT NULL,
    action TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
