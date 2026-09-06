-- A crawl that stops early is not a crawl that failed.
--
-- The courts registry walks the source's whole court list a page at a time.
-- When page 23 came back rate limited, the poll returned an error, the twenty-
-- two pages already read were thrown away, and the next poll started over at
-- page one — reached page 23, and was rate limited again. A feed that can only
-- ever fail says nothing about the courts it did read, and re-reading a list
-- the source already answered spends a public rate limit on nothing.
--
-- So a poll now ends one of three ways: it reached the end of the feed, it
-- stopped early and kept its place, or it failed outright. This column holds
-- the reason for the middle case. It is deliberately not `last_error`: someone
-- reading the sources page to decide whether ingestion is healthy needs to
-- tell "still working through a long list" from "broken".

ALTER TABLE ingest_cursors ADD COLUMN last_pause TEXT;

COMMENT ON COLUMN ingest_cursors.last_pause IS
    'Why the last poll stopped before the end of the feed, having kept the '
    'records it already read and a cursor to resume from. NULL when the poll '
    'reached the end. Not a failure: progress was made and saved.';

COMMENT ON COLUMN ingest_cursors.next_url IS
    'Where the next poll resumes. For a feed crawled to completion this holds '
    'a "complete:<timestamp>" marker rather than NULL, so the next poll can '
    'tell a finished crawl from one that never ran and decline to re-read the '
    'whole list. NULL means this feed has no saved place.';
