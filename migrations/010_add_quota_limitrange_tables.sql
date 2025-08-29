-- ResourceQuota table
CREATE TABLE IF NOT EXISTS resourcequotas (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    namespace TEXT NOT NULL,
    spec TEXT NOT NULL,
    status TEXT,
    hard TEXT,
    used TEXT,
    scope_selector TEXT,
    labels TEXT,
    annotations TEXT,
    resource_version INTEGER NOT NULL DEFAULT 1,
    generation INTEGER NOT NULL DEFAULT 1,
    creation_timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    deletion_timestamp DATETIME,
    UNIQUE(namespace, name)
);

CREATE INDEX idx_resourcequotas_namespace ON resourcequotas(namespace);
CREATE INDEX idx_resourcequotas_deletion ON resourcequotas(deletion_timestamp);

-- LimitRange table
CREATE TABLE IF NOT EXISTS limitranges (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    namespace TEXT NOT NULL,
    spec TEXT NOT NULL,
    limits TEXT NOT NULL,
    labels TEXT,
    annotations TEXT,
    resource_version INTEGER NOT NULL DEFAULT 1,
    generation INTEGER NOT NULL DEFAULT 1,
    creation_timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    deletion_timestamp DATETIME,
    UNIQUE(namespace, name)
);

CREATE INDEX idx_limitranges_namespace ON limitranges(namespace);
CREATE INDEX idx_limitranges_deletion ON limitranges(deletion_timestamp);