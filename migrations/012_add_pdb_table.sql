-- PodDisruptionBudget table
CREATE TABLE IF NOT EXISTS poddisruptionbudgets (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    namespace TEXT NOT NULL,
    spec TEXT NOT NULL,
    min_available TEXT,
    max_unavailable TEXT,
    selector TEXT NOT NULL,
    unhealthy_pod_eviction_policy TEXT,
    status TEXT,
    current_healthy INTEGER,
    desired_healthy INTEGER,
    disruptions_allowed INTEGER,
    expected_pods INTEGER,
    observed_generation INTEGER,
    labels TEXT,
    annotations TEXT,
    resource_version INTEGER NOT NULL DEFAULT 1,
    generation INTEGER NOT NULL DEFAULT 1,
    creation_timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    deletion_timestamp DATETIME,
    UNIQUE(namespace, name)
);

CREATE INDEX idx_pdb_namespace ON poddisruptionbudgets(namespace);
CREATE INDEX idx_pdb_deletion ON poddisruptionbudgets(deletion_timestamp);