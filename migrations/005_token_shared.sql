-- Allow user tokens to be shared into the pool for round-robin
ALTER TABLE user_kiro_tokens ADD COLUMN shared INTEGER NOT NULL DEFAULT 0;
