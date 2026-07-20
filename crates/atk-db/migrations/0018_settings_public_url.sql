-- Lets an operator pin the external URL used to build a repo's webhook payload URL, instead of
-- always inferring it from the connecting request's Host header (request_origin), which is
-- almost always a LAN address when the instance sits behind NAT. NULL keeps the old
-- auto-detect-from-request behavior.

ALTER TABLE settings ADD COLUMN public_url TEXT;
