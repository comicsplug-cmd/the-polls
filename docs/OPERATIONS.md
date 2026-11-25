# Operations runbook

## Production shape

One Linux VPS runs Caddy, the Next.js server, and Axum. Caddy owns HTTPS and forwards `/api/*` to Axum and other paths to Next.js. SQLite is stored on a local persistent filesystem in WAL mode. Do not place the database on a network filesystem.

Run each application service as a dedicated unprivileged account. Keep the API environment file readable only by root and the service account. It contains the Paystack secret (also used to verify the raw-body webhook signature), HMAC keys, admin Argon2 hash, and database URL. Never put secrets in the Next.js environment or build arguments.

## Monitoring and alerts

- Check `GET /api/health` every minute over HTTPS. It returns `200` with `degraded: true` when a payment is stuck for more than 15 minutes or a refund needs attention, and `503` only when the database is unavailable. Alert on `degraded` and on `503`.
- Use `GET /api/health/ready` as the load-balancer/readiness probe. It returns `503` only when the database is unreachable, so an operational warning never removes a healthy instance from rotation.
- The reconciliation worker runs every five minutes. It re-verifies pending payments and checks pending Paystack refund statuses without logging references or payloads.
- Forward application logs to the VPS monitoring service. Alert on health-check `503`, repeated payment reconciliation errors, a non-zero stuck-payment count, or refunds needing attention.
- Keep access logs disabled for `/api/paystack/callback` because the provider reference is a query parameter. Caddy's default is no access log; do not enable request-query logging.
- Add an external HTTPS uptime check for the public site and a webhook delivery alert in Paystack.

## Backups

Use `deploy/backup.sh` with a Restic repository on a separate provider. Restic encrypts repository contents. Store `RESTIC_REPOSITORY` and `RESTIC_PASSWORD_FILE` in root-owned environment/config files outside the app directory. Run nightly and retain 14 daily, 8 weekly, and 12 monthly snapshots.

At least monthly, restore a snapshot to an isolated directory, run `PRAGMA integrity_check`, start the API against the restored copy, and verify health, receipt status, and archived tally. Record the restore date and result. A backup that has not been restored is not verified.

The local SQLite online-backup behavior can be smoke-checked with `python scripts/test_backup_restore.py`; this does not replace restoring an encrypted off-host Restic snapshot on the production VPS.

## Admin credential rotation

`ADMIN_PASSWORD_HASH` is an Argon2 PHC string. Generate a new one with the API binary itself so the plaintext is read from stdin rather than an argument:

```sh
printf 'your-strong-password' | cargo run --manifest-path server/Cargo.toml -- --hash-password
```

Paste the printed string into the root-owned API environment file and restart the API; startup reseeds the admin row from the hash. The command refuses passwords shorter than 16 characters. Admin sign-in allows five attempts per ten minutes per client address, and existing sessions stay valid for up to eight hours.

## Payment incident notes

- Do not count from a browser callback or webhook body alone. The API calls Paystack Verify and uses its `paid_at` value inside the atomic settlement transaction.
- Do not copy payer details or raw webhooks into issue trackers or logs.
- A late, duplicate, mismatched, or otherwise uncountable successful payment is sent to Paystack for a full refund. The app does not collect bank details.
- If Paystack reports `needs-attention`, check the refund in Paystack and resolve it there. The app intentionally does not request customer bank details. Use the admin status check after Paystack updates it.
- Paystack refund requests use a stable idempotency reference. Check status before retrying; do not create a second refund while one is pending.
- Paystack fees on the original transaction are not returned. Replenish the refund reserve if needed.

## Privacy schedule

The hourly retention task archives candidate totals and detaches candidate/location links after close plus 60 days. After close plus one year, it deletes payment rows, card locks, old webhook event hashes, device/card HMACs, and terms acceptance timestamps. The receipt row remains with its poll, status, and time; aggregate tallies remain in the archive.

