-- Inputs own accepted launch choices; attempts retain their frozen snapshots.
-- NULL preferences identify pre-migration inputs whose original choices were lost.
ALTER TABLE public.session_inputs
    ADD COLUMN launch_preferences jsonb CHECK (
        launch_preferences IS NULL OR jsonb_typeof(launch_preferences) = 'object'),
    ADD COLUMN confirmed_launch jsonb CHECK (
        confirmed_launch IS NULL OR (launch_preferences IS NOT NULL
            AND jsonb_typeof(confirmed_launch) = 'object')),
    ADD COLUMN steering_flushed_at timestamptz CHECK (
        steering_flushed_at IS NULL OR steered);

COMMENT ON COLUMN public.session_inputs.steering_flushed_at IS
    'Positive steering RPC evidence for the current revision and delivery generation; ordered journal projection or settlement materializes its transcript.';
