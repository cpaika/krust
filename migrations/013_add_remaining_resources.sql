-- PriorityClass table (scheduling.k8s.io/v1)
CREATE TABLE IF NOT EXISTS priorityclasses (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    value INTEGER NOT NULL,
    global_default BOOLEAN DEFAULT FALSE,
    description TEXT,
    preemption_policy TEXT DEFAULT 'PreemptLowerPriority',
    labels TEXT,
    annotations TEXT,
    resource_version INTEGER NOT NULL DEFAULT 1,
    generation INTEGER NOT NULL DEFAULT 1,
    creation_timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    deletion_timestamp DATETIME
);

CREATE INDEX idx_priorityclasses_value ON priorityclasses(value);
CREATE INDEX idx_priorityclasses_deletion ON priorityclasses(deletion_timestamp);

-- StorageClass table (storage.k8s.io/v1)
CREATE TABLE IF NOT EXISTS storageclasses (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    provisioner TEXT NOT NULL,
    parameters TEXT,
    reclaim_policy TEXT DEFAULT 'Delete',
    mount_options TEXT,
    allow_volume_expansion BOOLEAN DEFAULT FALSE,
    volume_binding_mode TEXT DEFAULT 'Immediate',
    allowed_topologies TEXT,
    labels TEXT,
    annotations TEXT,
    resource_version INTEGER NOT NULL DEFAULT 1,
    generation INTEGER NOT NULL DEFAULT 1,
    creation_timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    deletion_timestamp DATETIME
);

CREATE INDEX idx_storageclasses_deletion ON storageclasses(deletion_timestamp);

-- ValidatingWebhookConfiguration table (admissionregistration.k8s.io/v1)
CREATE TABLE IF NOT EXISTS validatingwebhookconfigurations (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    webhooks TEXT NOT NULL,
    labels TEXT,
    annotations TEXT,
    resource_version INTEGER NOT NULL DEFAULT 1,
    generation INTEGER NOT NULL DEFAULT 1,
    creation_timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    deletion_timestamp DATETIME
);

CREATE INDEX idx_validatingwebhooks_deletion ON validatingwebhookconfigurations(deletion_timestamp);

-- MutatingWebhookConfiguration table (admissionregistration.k8s.io/v1)
CREATE TABLE IF NOT EXISTS mutatingwebhookconfigurations (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    webhooks TEXT NOT NULL,
    labels TEXT,
    annotations TEXT,
    resource_version INTEGER NOT NULL DEFAULT 1,
    generation INTEGER NOT NULL DEFAULT 1,
    creation_timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    deletion_timestamp DATETIME
);

CREATE INDEX idx_mutatingwebhooks_deletion ON mutatingwebhookconfigurations(deletion_timestamp);

-- CSIDriver table (storage.k8s.io/v1)
CREATE TABLE IF NOT EXISTS csidrivers (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    spec TEXT NOT NULL,
    attach_required BOOLEAN DEFAULT TRUE,
    pod_info_on_mount BOOLEAN DEFAULT FALSE,
    volume_lifecycle_modes TEXT,
    storage_capacity BOOLEAN DEFAULT FALSE,
    fs_group_policy TEXT,
    token_requests TEXT,
    requires_republish BOOLEAN DEFAULT FALSE,
    se_linux_mount BOOLEAN DEFAULT FALSE,
    labels TEXT,
    annotations TEXT,
    resource_version INTEGER NOT NULL DEFAULT 1,
    generation INTEGER NOT NULL DEFAULT 1,
    creation_timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    deletion_timestamp DATETIME
);

CREATE INDEX idx_csidrivers_deletion ON csidrivers(deletion_timestamp);

-- CSINode table (storage.k8s.io/v1)
CREATE TABLE IF NOT EXISTS csinodes (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    spec TEXT NOT NULL,
    drivers TEXT NOT NULL,
    labels TEXT,
    annotations TEXT,
    resource_version INTEGER NOT NULL DEFAULT 1,
    generation INTEGER NOT NULL DEFAULT 1,
    creation_timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    deletion_timestamp DATETIME
);

CREATE INDEX idx_csinodes_deletion ON csinodes(deletion_timestamp);

-- VolumeAttachment table (storage.k8s.io/v1)
CREATE TABLE IF NOT EXISTS volumeattachments (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    spec TEXT NOT NULL,
    attacher TEXT NOT NULL,
    source TEXT NOT NULL,
    node_name TEXT NOT NULL,
    status TEXT,
    attached BOOLEAN DEFAULT FALSE,
    attachment_metadata TEXT,
    attach_error TEXT,
    detach_error TEXT,
    labels TEXT,
    annotations TEXT,
    resource_version INTEGER NOT NULL DEFAULT 1,
    generation INTEGER NOT NULL DEFAULT 1,
    creation_timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    deletion_timestamp DATETIME
);

CREATE INDEX idx_volumeattachments_node ON volumeattachments(node_name);
CREATE INDEX idx_volumeattachments_deletion ON volumeattachments(deletion_timestamp);