-- Settlement ownership: a single worker may verify a payment at a time.
-- Reconciliation reclaims the claim after two minutes so a crashed worker cannot strand a ballot.
ALTER TABLE ballots ADD COLUMN verify_claim_at TEXT;

-- Public ballot code, derived from the verified Paystack transaction inside the
-- settlement transaction. It lets a voter find their ballot without exposing the
-- private payment reference or the candidate.
ALTER TABLE ballots ADD COLUMN vote_code TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS ballots_vote_code ON ballots(vote_code);
CREATE INDEX IF NOT EXISTS ballots_verify_claim ON ballots(status, verify_claim_at);
