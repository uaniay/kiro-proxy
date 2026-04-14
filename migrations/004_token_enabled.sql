-- Add enabled column to user_kiro_tokens for admin toggle
ALTER TABLE user_kiro_tokens ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1;
