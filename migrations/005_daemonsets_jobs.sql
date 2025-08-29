-- DaemonSet table
CREATE TABLE IF NOT EXISTS daemonsets (
    uid TEXT PRIMARY KEY,
    namespace TEXT NOT NULL,
    name TEXT NOT NULL,
    
    -- Spec fields
    selector TEXT NOT NULL, -- JSON object for label selector
    template TEXT NOT NULL, -- JSON object for pod template
    update_strategy TEXT NOT NULL, -- JSON object with type and rollingUpdate
    min_ready_seconds INTEGER DEFAULT 0,
    revision_history_limit INTEGER DEFAULT 10,
    
    -- Status fields
    current_number_scheduled INTEGER DEFAULT 0,
    number_misscheduled INTEGER DEFAULT 0,
    desired_number_scheduled INTEGER DEFAULT 0,
    number_ready INTEGER DEFAULT 0,
    observed_generation INTEGER DEFAULT 0,
    updated_number_scheduled INTEGER DEFAULT 0,
    number_available INTEGER DEFAULT 0,
    number_unavailable INTEGER DEFAULT 0,
    collision_count INTEGER DEFAULT 0,
    conditions TEXT, -- JSON array of conditions
    
    -- Metadata
    labels TEXT NOT NULL, -- JSON object
    annotations TEXT NOT NULL, -- JSON object
    resource_version INTEGER NOT NULL,
    generation INTEGER DEFAULT 1,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    
    UNIQUE(namespace, name)
);

-- Job table
CREATE TABLE IF NOT EXISTS jobs (
    uid TEXT PRIMARY KEY,
    namespace TEXT NOT NULL,
    name TEXT NOT NULL,
    
    -- Spec fields
    parallelism INTEGER DEFAULT 1,
    completions INTEGER,
    active_deadline_seconds INTEGER,
    backoff_limit INTEGER DEFAULT 6,
    selector TEXT, -- JSON object for label selector
    manual_selector BOOLEAN DEFAULT FALSE,
    template TEXT NOT NULL, -- JSON object for pod template
    ttl_seconds_after_finished INTEGER,
    completion_mode TEXT DEFAULT 'NonIndexed', -- NonIndexed or Indexed
    suspend BOOLEAN DEFAULT FALSE,
    
    -- Status fields
    conditions TEXT, -- JSON array of conditions
    start_time TEXT,
    completion_time TEXT,
    active INTEGER DEFAULT 0,
    succeeded INTEGER DEFAULT 0,
    failed INTEGER DEFAULT 0,
    completed_indexes TEXT, -- For Indexed jobs
    uncounted_terminated_pods TEXT, -- JSON object
    ready INTEGER DEFAULT 0,
    
    -- Metadata
    labels TEXT NOT NULL, -- JSON object
    annotations TEXT NOT NULL, -- JSON object
    resource_version INTEGER NOT NULL,
    generation INTEGER DEFAULT 1,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    
    UNIQUE(namespace, name)
);

-- CronJob table
CREATE TABLE IF NOT EXISTS cronjobs (
    uid TEXT PRIMARY KEY,
    namespace TEXT NOT NULL,
    name TEXT NOT NULL,
    
    -- Spec fields
    schedule TEXT NOT NULL, -- Cron expression
    timezone TEXT, -- Timezone for schedule
    starting_deadline_seconds INTEGER,
    concurrency_policy TEXT DEFAULT 'Allow', -- Allow, Forbid, or Replace
    suspend BOOLEAN DEFAULT FALSE,
    job_template TEXT NOT NULL, -- JSON object for job template
    successful_jobs_history_limit INTEGER DEFAULT 3,
    failed_jobs_history_limit INTEGER DEFAULT 1,
    
    -- Status fields
    active TEXT, -- JSON array of active job references
    last_schedule_time TEXT,
    last_successful_time TEXT,
    
    -- Metadata
    labels TEXT NOT NULL, -- JSON object
    annotations TEXT NOT NULL, -- JSON object
    resource_version INTEGER NOT NULL,
    generation INTEGER DEFAULT 1,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    
    UNIQUE(namespace, name)
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_daemonset_namespace ON daemonsets(namespace);
CREATE INDEX IF NOT EXISTS idx_job_namespace ON jobs(namespace);
CREATE INDEX IF NOT EXISTS idx_job_completion_time ON jobs(completion_time);
CREATE INDEX IF NOT EXISTS idx_cronjob_namespace ON cronjobs(namespace);
CREATE INDEX IF NOT EXISTS idx_cronjob_schedule ON cronjobs(schedule);