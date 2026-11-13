-- Onboarding tour progress (users g5.user.onboarding): the highest tour step
-- the person reached (null until the welcome card is shown, 7 when finished)
-- and whether they skipped the tour. Both belong to the person, not the
-- browser, so drop-off and skip counts can be read across users.
ALTER TABLE public.users
    ADD COLUMN onboarding_step smallint
        CONSTRAINT users_onboarding_step_range CHECK (onboarding_step BETWEEN 0 AND 7),
    ADD COLUMN onboarding_skipped boolean NOT NULL DEFAULT false;
