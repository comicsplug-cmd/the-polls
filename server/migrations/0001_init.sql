PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS elections (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  slug TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL,
  region TEXT NOT NULL,
  election_date TEXT NOT NULL,
  open_at TEXT NOT NULL,
  close_at TEXT NOT NULL,
  included INTEGER NOT NULL DEFAULT 0,
  roster_status TEXT NOT NULL,
  source_url TEXT NOT NULL,
  source_label TEXT NOT NULL,
  force_closed_at TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS candidates (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  election_id INTEGER NOT NULL REFERENCES elections(id) ON DELETE CASCADE,
  party TEXT NOT NULL,
  name TEXT NOT NULL,
  running_mate TEXT,
  UNIQUE(election_id, party)
);

CREATE TABLE IF NOT EXISTS ballots (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  receipt_id TEXT NOT NULL UNIQUE,
  election_id INTEGER NOT NULL REFERENCES elections(id),
  candidate_id INTEGER REFERENCES candidates(id),
  location TEXT,
  status TEXT NOT NULL CHECK(status IN ('pending','verifying','counted','refund_pending','refunding','refunded','refund_failed','abandoned')),
  accepted_terms_version TEXT,
  accepted_terms_at TEXT,
  card_fingerprint TEXT,
  device_fingerprint TEXT,
  created_at TEXT NOT NULL,
  counted_at TEXT,
  detached_at TEXT
);
CREATE INDEX IF NOT EXISTS ballots_election_status ON ballots(election_id, status);
CREATE INDEX IF NOT EXISTS ballots_candidate_status ON ballots(candidate_id, status);

CREATE TABLE IF NOT EXISTS payments (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ballot_id INTEGER NOT NULL UNIQUE REFERENCES ballots(id),
  reference TEXT UNIQUE,
  base_amount_kobo INTEGER NOT NULL,
  requested_amount_kobo INTEGER NOT NULL,
  actual_amount_kobo INTEGER,
  fee_kobo INTEGER,
  currency TEXT NOT NULL DEFAULT 'NGN',
  channel TEXT NOT NULL,
  provider_transaction_id TEXT UNIQUE,
  paid_at TEXT,
  transfer_expires_at TEXT,
  authorization_hash TEXT,
  refund_reference TEXT UNIQUE,
  refund_status TEXT,
  refund_last_checked_at TEXT,
  support_purge_at TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS payments_refund_status ON payments(refund_status);

CREATE TABLE IF NOT EXISTS card_locks (
  election_id INTEGER NOT NULL REFERENCES elections(id),
  card_fingerprint TEXT NOT NULL,
  ballot_id INTEGER NOT NULL UNIQUE REFERENCES ballots(id),
  created_at TEXT NOT NULL,
  PRIMARY KEY(election_id, card_fingerprint)
);

CREATE TABLE IF NOT EXISTS webhook_events (
  event_key TEXT PRIMARY KEY,
  event_type TEXT NOT NULL,
  received_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS admins (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  username TEXT NOT NULL UNIQUE,
  password_hash TEXT NOT NULL,
  active INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE IF NOT EXISTS admin_sessions (
  token_hash TEXT PRIMARY KEY,
  admin_id INTEGER NOT NULL REFERENCES admins(id) ON DELETE CASCADE,
  csrf_hash TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS admin_audit (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  admin_id INTEGER NOT NULL REFERENCES admins(id),
  action TEXT NOT NULL,
  target TEXT NOT NULL,
  created_at TEXT NOT NULL,
  detail TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS tally_archives (
  election_id INTEGER NOT NULL REFERENCES elections(id),
  candidate_label TEXT NOT NULL,
  party TEXT NOT NULL,
  total INTEGER NOT NULL,
  archived_at TEXT NOT NULL,
  PRIMARY KEY(election_id, party)
);

CREATE TABLE IF NOT EXISTS privacy_purge_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  detached_ballots INTEGER NOT NULL,
  purged_payments INTEGER NOT NULL,
  ran_at TEXT NOT NULL
);

