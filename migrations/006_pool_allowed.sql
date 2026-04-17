ALTER TABLE users ADD COLUMN pool_allowed INTEGER NOT NULL DEFAULT 0;
UPDATE users SET pool_allowed = 1 WHERE role = 'admin';
