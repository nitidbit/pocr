-- Study lifecycle gains Paused and replaces the old catch-all 'inactive'
-- with the terminal 'closed'. Map any pre-existing 'inactive' studies to
-- 'closed' before swapping the CHECK constraint.
UPDATE studies SET status = 'closed' WHERE status = 'inactive';

ALTER TABLE studies DROP CONSTRAINT studies_status_check;
ALTER TABLE studies
    ADD CONSTRAINT studies_status_check
        CHECK (status IN ('draft', 'active', 'paused', 'closed'));
