-- Add generation column to replicasets table if it doesn't exist
ALTER TABLE replicasets ADD COLUMN generation INTEGER DEFAULT 1;