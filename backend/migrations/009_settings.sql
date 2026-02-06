-- Application settings table
CREATE TABLE IF NOT EXISTS app_settings (
    key VARCHAR(255) PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Insert default title
INSERT INTO app_settings (key, value) VALUES ('site_title', 'NAVIDROME RADIO')
ON CONFLICT (key) DO NOTHING;
