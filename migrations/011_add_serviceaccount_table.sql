-- ServiceAccount table
CREATE TABLE IF NOT EXISTS serviceaccounts (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    namespace TEXT NOT NULL,
    secrets TEXT,
    image_pull_secrets TEXT,
    automount_service_account_token BOOLEAN DEFAULT TRUE,
    labels TEXT,
    annotations TEXT,
    resource_version INTEGER NOT NULL DEFAULT 1,
    generation INTEGER NOT NULL DEFAULT 1,
    creation_timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    deletion_timestamp DATETIME,
    UNIQUE(namespace, name)
);

CREATE INDEX idx_serviceaccounts_namespace ON serviceaccounts(namespace);
CREATE INDEX idx_serviceaccounts_deletion ON serviceaccounts(deletion_timestamp);

-- TokenRequest table for ServiceAccount tokens
CREATE TABLE IF NOT EXISTS tokenrequests (
    uid TEXT PRIMARY KEY,
    service_account_uid TEXT NOT NULL,
    namespace TEXT NOT NULL,
    service_account_name TEXT NOT NULL,
    audiences TEXT,
    expiration_seconds INTEGER,
    bound_object_ref TEXT,
    token TEXT NOT NULL,
    status TEXT,
    creation_timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    expiration_timestamp DATETIME,
    FOREIGN KEY (service_account_uid) REFERENCES serviceaccounts(uid)
);

CREATE INDEX idx_tokenrequests_sa ON tokenrequests(service_account_uid);
CREATE INDEX idx_tokenrequests_expiration ON tokenrequests(expiration_timestamp);