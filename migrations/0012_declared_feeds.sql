-- Which feeds are being polled is known by the process doing the polling.
--
-- The sources page asked the API, and the API answered from its own
-- environment. In a deployment where the API and the ingestion service have
-- separate configuration -- which is how docker-compose defines them, and how
-- they were run -- the API reported a feed it had never heard of as "not
-- configured" while that feed brought in new records every two minutes.
--
-- A transparency page describing a live feed as switched off is worse than one
-- that omits it: a reader looking for coverage gaps would find a gap that is
-- not there and miss that the records exist. So the ingestion service now
-- declares its feed list here, and anyone reporting on coverage reads it from
-- the poller rather than guessing from their own environment.

ALTER TABLE ingest_cursors ADD COLUMN configured BOOLEAN;

COMMENT ON COLUMN ingest_cursors.configured IS
    'Whether the ingestion service is currently configured to poll this feed, '
    'as declared by that service on startup. NULL means no service has '
    'declared a list yet, and a reader should fall back to its own '
    'configuration; false means a feed with history that is no longer polled.';
