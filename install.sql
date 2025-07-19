-- QML PostgreSQL Schema Installation
-- Complete schema for QML job queue system with distributed job locking
-- This file contains the complete PostgreSQL schema required for QML

-- =========================================================================
-- SCHEMA AND TABLE CREATION
-- =========================================================================

-- Create schema if it doesn't exist
CREATE SCHEMA IF NOT EXISTS qml;

-- Create the main jobs table
CREATE TABLE IF NOT EXISTS qml.qml_jobs (
    -- Primary job identification
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    method_name VARCHAR(255) NOT NULL,
    arguments JSONB NOT NULL DEFAULT '[]'::jsonb,
    
    -- Job state management
    state_name VARCHAR(50) NOT NULL DEFAULT 'pending',
    state_data JSONB NOT NULL DEFAULT '{}'::jsonb,
    
    -- Queue and priority management
    queue_name VARCHAR(255) NOT NULL DEFAULT 'default',
    priority INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    
    -- Additional data and metadata
    metadata JSONB DEFAULT NULL,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    scheduled_for TIMESTAMPTZ DEFAULT NULL,
    expires_at TIMESTAMPTZ DEFAULT NULL,
    
    -- Distributed job locking (for multi-worker environments)
    locked_by VARCHAR(255) DEFAULT NULL,
    locked_at TIMESTAMPTZ DEFAULT NULL,
    lock_expires_at TIMESTAMPTZ DEFAULT NULL
);

-- =========================================================================
-- PERFORMANCE INDEXES
-- =========================================================================

-- Core performance indexes for job processing
CREATE INDEX IF NOT EXISTS idx_qml_jobs_state_name ON qml.qml_jobs(state_name);
CREATE INDEX IF NOT EXISTS idx_qml_jobs_queue_name ON qml.qml_jobs(queue_name);
CREATE INDEX IF NOT EXISTS idx_qml_jobs_priority ON qml.qml_jobs(priority DESC);
CREATE INDEX IF NOT EXISTS idx_qml_jobs_created_at ON qml.qml_jobs(created_at);

-- Composite indexes for efficient job fetching
CREATE INDEX IF NOT EXISTS idx_qml_jobs_state_queue ON qml.qml_jobs(state_name, queue_name);
CREATE INDEX IF NOT EXISTS idx_qml_jobs_state_priority ON qml.qml_jobs(state_name, priority DESC);
CREATE INDEX IF NOT EXISTS idx_qml_jobs_queue_priority ON qml.qml_jobs(queue_name, priority DESC);

-- Scheduling and cleanup indexes
CREATE INDEX IF NOT EXISTS idx_qml_jobs_scheduled_for ON qml.qml_jobs(scheduled_for) WHERE scheduled_for IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_qml_jobs_expires_at ON qml.qml_jobs(expires_at) WHERE expires_at IS NOT NULL;

-- Distributed locking indexes
CREATE INDEX IF NOT EXISTS idx_qml_jobs_locked_by ON qml.qml_jobs(locked_by) WHERE locked_by IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_qml_jobs_lock_expires ON qml.qml_jobs(lock_expires_at) WHERE lock_expires_at IS NOT NULL;

-- =========================================================================
-- TRIGGERS AND FUNCTIONS
-- =========================================================================

-- Function to automatically update the updated_at timestamp
CREATE OR REPLACE FUNCTION qml.update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE 'plpgsql';

-- Trigger to automatically update updated_at on row changes
DROP TRIGGER IF EXISTS trigger_update_qml_jobs_updated_at ON qml.qml_jobs;
CREATE TRIGGER trigger_update_qml_jobs_updated_at
    BEFORE UPDATE ON qml.qml_jobs
    FOR EACH ROW
    EXECUTE FUNCTION qml.update_updated_at_column();

-- =========================================================================
-- JOB STATE ENUM (Optional - for type safety)
-- =========================================================================

-- Create job state enum type for better type safety
DO $$ BEGIN
    CREATE TYPE qml.job_state AS ENUM (
        'pending',      -- Job is waiting to be processed
        'running',      -- Job is currently being processed
        'completed',    -- Job completed successfully
        'failed',       -- Job failed and won't be retried
        'cancelled',    -- Job was cancelled
        'retrying',     -- Job failed but will be retried
        'scheduled'     -- Job is scheduled for future execution
    );
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

-- =========================================================================
-- DISTRIBUTED JOB LOCKING FUNCTIONS
-- =========================================================================

-- Function to atomically acquire a job lock
CREATE OR REPLACE FUNCTION qml.acquire_job_lock(
    p_job_id UUID,
    p_worker_id VARCHAR(255),
    p_lock_duration INTERVAL DEFAULT '5 minutes'::interval
) RETURNS BOOLEAN AS $$
DECLARE
    rows_affected INTEGER;
BEGIN
    -- Try to acquire lock on the job (atomic operation)
    UPDATE qml.qml_jobs 
    SET 
        locked_by = p_worker_id,
        locked_at = NOW(),
        lock_expires_at = NOW() + p_lock_duration
    WHERE 
        id = p_job_id 
        AND (
            locked_by IS NULL 
            OR lock_expires_at < NOW()  -- Lock has expired
        );
    
    GET DIAGNOSTICS rows_affected = ROW_COUNT;
    RETURN rows_affected > 0;
END;
$$ LANGUAGE plpgsql;

-- Function to release a job lock
CREATE OR REPLACE FUNCTION qml.release_job_lock(
    p_job_id UUID,
    p_worker_id VARCHAR(255)
) RETURNS BOOLEAN AS $$
DECLARE
    rows_affected INTEGER;
BEGIN
    -- Only release if the worker actually owns the lock
    UPDATE qml.qml_jobs 
    SET 
        locked_by = NULL,
        locked_at = NULL,
        lock_expires_at = NULL
    WHERE 
        id = p_job_id 
        AND locked_by = p_worker_id;
    
    GET DIAGNOSTICS rows_affected = ROW_COUNT;
    RETURN rows_affected > 0;
END;
$$ LANGUAGE plpgsql;

-- Function to cleanup expired locks (maintenance function)
CREATE OR REPLACE FUNCTION qml.cleanup_expired_locks() RETURNS INTEGER AS $$
DECLARE
    rows_affected INTEGER;
BEGIN
    UPDATE qml.qml_jobs 
    SET 
        locked_by = NULL,
        locked_at = NULL,
        lock_expires_at = NULL
    WHERE 
        lock_expires_at < NOW();
    
    GET DIAGNOSTICS rows_affected = ROW_COUNT;
    RETURN rows_affected;
END;
$$ LANGUAGE plpgsql;

-- Function to get next available job with locking
CREATE OR REPLACE FUNCTION qml.get_next_job(
    p_worker_id VARCHAR(255),
    p_queue_names VARCHAR(255)[] DEFAULT ARRAY['default'],
    p_lock_duration INTERVAL DEFAULT '5 minutes'::interval
) RETURNS TABLE(job_id UUID, method_name VARCHAR, arguments JSONB) AS $$
DECLARE
    selected_job_id UUID;
BEGIN
    -- Find and lock the next available job atomically
    SELECT id INTO selected_job_id
    FROM qml.qml_jobs
    WHERE 
        state_name = 'pending'
        AND queue_name = ANY(p_queue_names)
        AND (scheduled_for IS NULL OR scheduled_for <= NOW())
        AND (locked_by IS NULL OR lock_expires_at < NOW())
    ORDER BY priority DESC, created_at ASC
    LIMIT 1
    FOR UPDATE SKIP LOCKED;
    
    -- If we found a job, try to acquire the lock
    IF selected_job_id IS NOT NULL THEN
        IF qml.acquire_job_lock(selected_job_id, p_worker_id, p_lock_duration) THEN
            -- Return the job details
            RETURN QUERY
            SELECT j.id, j.method_name, j.arguments
            FROM qml.qml_jobs j
            WHERE j.id = selected_job_id;
        END IF;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- =========================================================================
-- TABLE AND COLUMN DOCUMENTATION
-- =========================================================================

-- Table documentation
COMMENT ON TABLE qml.qml_jobs IS 'QML job queue table for storing and managing background jobs';

-- Column documentation
COMMENT ON COLUMN qml.qml_jobs.id IS 'Unique identifier for the job (UUID)';
COMMENT ON COLUMN qml.qml_jobs.method_name IS 'Name of the method/worker to execute';
COMMENT ON COLUMN qml.qml_jobs.arguments IS 'JSON array of method arguments';
COMMENT ON COLUMN qml.qml_jobs.state_name IS 'Current processing state of the job';
COMMENT ON COLUMN qml.qml_jobs.state_data IS 'Additional state-specific data (JSON)';
COMMENT ON COLUMN qml.qml_jobs.queue_name IS 'Queue name for job organization and routing';
COMMENT ON COLUMN qml.qml_jobs.priority IS 'Job priority (higher number = higher priority)';
COMMENT ON COLUMN qml.qml_jobs.max_retries IS 'Maximum number of retry attempts allowed';
COMMENT ON COLUMN qml.qml_jobs.metadata IS 'Additional job metadata and context (JSON)';
COMMENT ON COLUMN qml.qml_jobs.created_at IS 'Timestamp when the job was created';
COMMENT ON COLUMN qml.qml_jobs.updated_at IS 'Timestamp when the job was last updated (auto-updated)';
COMMENT ON COLUMN qml.qml_jobs.scheduled_for IS 'When the job should be processed (for delayed jobs)';
COMMENT ON COLUMN qml.qml_jobs.expires_at IS 'When the job expires and should be cleaned up';
COMMENT ON COLUMN qml.qml_jobs.locked_by IS 'Worker ID that currently has this job locked';
COMMENT ON COLUMN qml.qml_jobs.locked_at IS 'Timestamp when the job lock was acquired';
COMMENT ON COLUMN qml.qml_jobs.lock_expires_at IS 'When the current job lock expires';

-- Function documentation
COMMENT ON FUNCTION qml.acquire_job_lock IS 'Atomically acquire a distributed lock on a job';
COMMENT ON FUNCTION qml.release_job_lock IS 'Release a job lock held by a specific worker';
COMMENT ON FUNCTION qml.cleanup_expired_locks IS 'Clean up all expired job locks (maintenance)';
COMMENT ON FUNCTION qml.get_next_job IS 'Get and lock the next available job for processing';
COMMENT ON FUNCTION qml.update_updated_at_column IS 'Trigger function to automatically update updated_at timestamp';

-- =========================================================================
-- SCHEMA INSTALLATION COMPLETE
-- =========================================================================

-- Log successful installation
DO $$
BEGIN
    RAISE NOTICE 'QML PostgreSQL schema installation completed successfully';
    RAISE NOTICE 'Schema: qml';
    RAISE NOTICE 'Tables: qml_jobs';
    RAISE NOTICE 'Functions: acquire_job_lock, release_job_lock, cleanup_expired_locks, get_next_job';
    RAISE NOTICE 'Triggers: automatic updated_at timestamp';
    RAISE NOTICE 'Ready for production job processing with distributed locking support';
END
$$;
