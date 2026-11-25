use std::{
    collections::HashMap,
    env,
    net::SocketAddr,
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use argon2::PasswordVerifier;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use hmac::{Hmac, Mac};
use rand::random;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256, Sha512};
use sqlx::{FromRow, SqlitePool, sqlite::SqliteConnectOptions};
use subtle::ConstantTimeEq;
use tokio::time::interval;
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

type HmacSha256 = Hmac<Sha256>;
type HmacSha512 = Hmac<Sha512>;

#[derive(Clone)]
struct AppState {
    db: SqlitePool,
    http: Client,
    config: Config,
    limits: RateLimits,
}

/// Small in-process sliding-window limiter. One API process owns the buckets,
/// and restarting the process clears them. That is enough to stop accidental
/// card-testing loops, ballot spam, and admin password guessing without adding
/// Redis to a single-box deployment.
#[derive(Clone, Default)]
struct RateLimits {
    buckets: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
}

impl RateLimits {
    fn allow(&self, key: &str, max: usize, window: Duration) -> bool {
        let now = Instant::now();
        let mut guard = match self.buckets.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if guard.len() > 20_000 {
            guard.retain(|_, hits| {
                hits.retain(|at| now.duration_since(*at) < window);
                !hits.is_empty()
            });
        }
        let hits = guard.entry(key.to_owned()).or_default();
        hits.retain(|at| now.duration_since(*at) < window);
        if hits.len() >= max {
            return false;
        }
        hits.push(now);
        true
    }
}

#[derive(Clone)]
struct Config {
    bind: String,
    database_url: String,
    public_base_url: String,
    api_public_url: String,
    paystack_api_base: String,
    paystack_secret: Option<String>,
    synthetic_domain: String,
    device_hmac_key: Option<String>,
    card_fingerprint_hmac_key: Option<String>,
    admin_password_hash: Option<String>,
    admin_session_hmac_key: Option<String>,
    admin_username: String,
    cookie_secure: bool,
    paid_polling_enabled: bool,
    demo_mode: bool,
    fee_pass_through_confirmed: bool,
    legal_review_approved: bool,
    paystack_live_approved: bool,
    roster_verification_approved: bool,
    payment_refund_rehearsals_complete: bool,
    production_domain_configured: bool,
    backups_and_restore_verified: bool,
    refund_reserve_ready: bool,
    admin_controls_verified: bool,
}

impl Config {
    fn from_env() -> Self {
        Self {
            bind: env::var("API_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into()),
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://./server/data/polls.sqlite?mode=rwc".into()),
            public_base_url: env::var("PUBLIC_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:3000".into()),
            api_public_url: env::var("API_PUBLIC_URL")
                .unwrap_or_else(|_| "http://localhost:8080".into()),
            paystack_api_base: env::var("PAYSTACK_API_BASE")
                .unwrap_or_else(|_| "https://api.paystack.co".into()),
            paystack_secret: nonempty_env("PAYSTACK_SECRET_KEY"),
            synthetic_domain: env::var("PAYSTACK_SYNTHETIC_EMAIL_DOMAIN")
                .unwrap_or_else(|_| "receipts.invalid".into()),
            device_hmac_key: nonempty_env("DEVICE_FINGERPRINT_HMAC_KEY"),
            card_fingerprint_hmac_key: nonempty_env("CARD_FINGERPRINT_HMAC_KEY"),
            admin_password_hash: nonempty_env("ADMIN_PASSWORD_HASH"),
            admin_session_hmac_key: nonempty_env("ADMIN_SESSION_HMAC_KEY"),
            admin_username: env::var("ADMIN_USERNAME").unwrap_or_else(|_| "admin".into()),
            cookie_secure: env_bool("COOKIE_SECURE", false),
            paid_polling_enabled: env_bool("PAID_POLLING_ENABLED", false),
            demo_mode: env_bool("DEMO_MODE", false),
            fee_pass_through_confirmed: env_bool("PAYSTACK_FEE_PASS_THROUGH_CONFIRMED", false),
            legal_review_approved: env_bool("LEGAL_REVIEW_APPROVED", false),
            paystack_live_approved: env_bool("PAYSTACK_LIVE_APPROVED", false),
            roster_verification_approved: env_bool("ROSTER_VERIFICATION_APPROVED", false),
            payment_refund_rehearsals_complete: env_bool(
                "PAYMENT_REFUND_REHEARSALS_COMPLETE",
                false,
            ),
            production_domain_configured: env_bool("PRODUCTION_DOMAIN_CONFIGURED", false),
            backups_and_restore_verified: env_bool("BACKUPS_AND_RESTORE_VERIFIED", false),
            refund_reserve_ready: env_bool("REFUND_RESERVE_READY", false),
            admin_controls_verified: env_bool("ADMIN_CONTROLS_VERIFIED", false),
        }
    }

    fn launch_gates_complete(&self) -> bool {
        self.legal_review_approved
            && self.paystack_live_approved
            && self.roster_verification_approved
            && self.payment_refund_rehearsals_complete
            && self.production_domain_configured
            && self.backups_and_restore_verified
            && self.refund_reserve_ready
            && self.admin_controls_verified
            && self.paystack_secret.is_some()
            && self.device_hmac_key.is_some()
            && self.card_fingerprint_hmac_key.is_some()
            && self.admin_password_hash.is_some()
            && self.admin_session_hmac_key.is_some()
            && self.cookie_secure
            && self.public_base_url.starts_with("https://")
            && self.api_public_url.starts_with("https://")
            && (!TERMS_VERSION.contains("draft") || cfg!(test))
            && self
                .paystack_secret
                .as_deref()
                .is_some_and(|key| key.starts_with("sk_live_"))
    }

    /// Whether checkout may start. Live mode needs every launch gate. Demo mode
    /// is a deliberate, test-key-only path so the whole payment → ballot →
    /// receipt loop can be rehearsed without pretending the product is live.
    fn payments_ready(&self) -> bool {
        if !self.paid_polling_enabled || !self.fee_pass_through_confirmed {
            return false;
        }
        if self.paystack_secret.is_none()
            || self.device_hmac_key.is_none()
            || self.card_fingerprint_hmac_key.is_none()
        {
            return false;
        }
        if self.demo_mode {
            return self
                .paystack_secret
                .as_deref()
                .is_some_and(|key| key.starts_with("sk_test_"));
        }
        self.launch_gates_complete()
    }

    fn mode_label(&self) -> &'static str {
        if self.demo_mode { "test" } else { "live" }
    }
}

#[derive(Debug, Serialize)]
struct ApiError {
    error: String,
}

impl ApiError {
    fn response(status: StatusCode, message: impl Into<String>) -> Response {
        (
            status,
            Json(Self {
                error: message.into(),
            }),
        )
            .into_response()
    }
}

#[derive(FromRow, Clone)]
struct ElectionRow {
    id: i64,
    slug: String,
    title: String,
    region: String,
    election_date: String,
    open_at: String,
    close_at: String,
    included: i64,
    roster_status: String,
    source_url: String,
    source_label: String,
    force_closed_at: Option<String>,
}

#[derive(FromRow, Serialize)]
struct CandidateRow {
    id: i64,
    party: String,
    name: String,
    running_mate: Option<String>,
}

#[derive(Serialize)]
struct ElectionPublic {
    slug: String,
    title: String,
    region: String,
    election_date: String,
    open_at: String,
    close_at: String,
    server_now: String,
    included: bool,
    roster_status: String,
    source_url: String,
    source_label: String,
    is_open: bool,
    demo_mode: bool,
    candidates: Vec<CandidateRow>,
}

#[derive(Serialize)]
struct TallyCandidate {
    party: String,
    name: String,
    running_mate: Option<String>,
    total: i64,
}

#[derive(Serialize)]
struct TallyResponse {
    election: String,
    total_counted: i64,
    updated_at: String,
    results: Vec<TallyCandidate>,
}

#[derive(Serialize)]
struct ReceiptPublic {
    receipt_id: String,
    ballot_code: Option<String>,
    poll: String,
    status: String,
    created_at: String,
    counted_at: Option<String>,
}

#[derive(Deserialize)]
struct VoteInitRequest {
    election: String,
    candidate_party: String,
    #[serde(default)]
    country_code: Option<String>,
    #[serde(default)]
    state_code: Option<String>,
    accepted_terms: bool,
    terms_version: String,
    channel: String,
}

#[derive(Serialize)]
struct VoteInitResponse {
    receipt_id: String,
    channel: String,
    base_amount_kobo: i64,
    requested_amount_kobo: i64,
    estimated_fee_kobo: i64,
    checkout_url: Option<String>,
    transfer: Option<TransferInstructions>,
}

#[derive(Serialize)]
struct TransferInstructions {
    bank: Option<String>,
    account_name: Option<String>,
    account_number: Option<String>,
    expires_at: Option<String>,
    message: String,
}

#[derive(Deserialize)]
struct PaystackEnvelope<T> {
    status: bool,
    #[serde(default)]
    _message: Option<String>,
    data: Option<T>,
}

#[derive(Deserialize)]
struct InitData {
    authorization_url: Option<String>,
    reference: Option<String>,
    display_text: Option<String>,
    #[serde(default)]
    bank: Option<Value>,
    #[serde(default)]
    account_number: Option<String>,
    #[serde(default)]
    account_name: Option<String>,
    #[serde(default)]
    expires_at: Option<String>,
    #[serde(default)]
    account_expires_at: Option<String>,
}

#[derive(Deserialize, Clone)]
struct VerifiedTransaction {
    id: i64,
    reference: String,
    status: String,
    amount: i64,
    currency: Option<String>,
    paid_at: Option<String>,
    channel: Option<String>,
    fees: Option<i64>,
    authorization: Option<AuthData>,
}

#[derive(Deserialize, Clone)]
struct AuthData {
    signature: Option<String>,
}

#[derive(FromRow)]
struct SettlementRow {
    ballot_id: i64,
    election_id: i64,
    status: String,
    base_amount_kobo: i64,
    requested_amount_kobo: i64,
    channel: String,
}

#[derive(FromRow)]
struct ReceiptRow {
    receipt_id: String,
    vote_code: Option<String>,
    title: String,
    status: String,
    created_at: String,
    counted_at: Option<String>,
}

#[derive(Deserialize)]
struct CallbackQuery {
    reference: Option<String>,
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct LookupRequest {
    reference: String,
}

#[derive(Deserialize)]
struct RefundRequest {
    receipt_id: String,
}

#[derive(Deserialize)]
struct CloseRequest {
    election: String,
}

#[derive(FromRow, Serialize)]
struct RefundQueueRow {
    receipt_id: String,
    election: String,
    status: String,
    actual_amount_kobo: Option<i64>,
    refund_status: Option<String>,
    refund_reference: Option<String>,
}

#[derive(FromRow, Serialize)]
struct AdminElectionRow {
    slug: String,
    title: String,
    open_at: String,
    close_at: String,
    included: i64,
    roster_status: String,
    force_closed_at: Option<String>,
    counted: i64,
    awaiting_payment: i64,
    refunds: i64,
}

#[derive(Debug)]
enum SettlementResult {
    Counted,
    RefundRequired,
    NoChange,
    DuplicateEvent,
}

const BASE_AMOUNT_KOBO: i64 = 10_000;
const TERMS_VERSION: &str = "2026-11-25-draft";

/// Reads a password from stdin and prints the Argon2 PHC string for
/// `ADMIN_PASSWORD_HASH`. A 16-character minimum matches the policy in
/// `scripts/make_admin_hash.py`, but this path needs no Python packages.
fn hash_password_command() -> Result<(), Box<dyn std::error::Error>> {
    use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
    use std::io::Read;

    let mut password = String::new();
    std::io::stdin().read_to_string(&mut password)?;
    let password = password.trim_end_matches(['\r', '\n']);
    if password.chars().count() < 16 {
        eprintln!("Refusing to hash a password shorter than 16 characters.");
        std::process::exit(2);
    }
    // ThreadRng is a CSPRNG; 16 random bytes are a valid Argon2 salt.
    let salt =
        SaltString::encode_b64(&random::<[u8; 16]>()).map_err(|error| format!("Salt: {error}"))?;
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|error| format!("Could not hash the password: {error}"))?;
    println!("{hash}");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();
    // `--hash-password` turns an operator-supplied password into the Argon2 PHC
    // string that `ADMIN_PASSWORD_HASH` expects. The password is read from stdin
    // so it never lands in shell history or the process list.
    if env::args().nth(1).as_deref() == Some("--hash-password") {
        return hash_password_command();
    }
    let config = Config::from_env();
    if config.database_url.starts_with("sqlite://./server/data/") {
        std::fs::create_dir_all("./server/data")?;
    }
    let db = connect_db(&config.database_url).await?;
    sqlx::migrate!("./migrations").run(&db).await?;
    seed_elections(&db).await?;
    seed_admin(&db, &config).await?;

    let state = AppState {
        db,
        http: Client::builder().timeout(Duration::from_secs(15)).build()?,
        config: config.clone(),
        limits: RateLimits::default(),
    };
    let cors_origin = HeaderValue::from_str(config.public_base_url.trim_end_matches('/'))?;
    let cors = CorsLayer::new()
        .allow_origin(cors_origin)
        .allow_credentials(true)
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            "x-csrf-token".parse().unwrap(),
            "x-device-token".parse().unwrap(),
        ]);

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/health/ready", get(health_ready))
        .route("/api/config", get(public_config))
        .route("/api/terms", get(terms))
        .route("/api/elections", get(list_elections))
        .route("/api/elections/{slug}", get(get_election))
        .route("/api/elections/{slug}/tally", get(get_tally))
        .route("/api/vote/init", post(init_vote))
        .route("/api/receipt/{receipt_id}", get(get_receipt))
        .route("/api/paystack/webhook", post(paystack_webhook))
        .route("/api/paystack/callback", get(paystack_callback))
        .route("/api/admin/login", post(admin_login))
        .route("/api/admin/logout", post(admin_logout))
        .route("/api/admin/refunds", get(admin_refunds))
        .route("/api/admin/overview", get(admin_overview))
        .route("/api/admin/risk-signals", get(admin_risk_signals))
        .route("/api/admin/lookup", post(admin_lookup))
        .route("/api/admin/refund", post(admin_refund))
        .route("/api/admin/refund-check", post(admin_refund_check))
        .route("/api/admin/force-close", post(admin_force_close))
        .layer(RequestBodyLimitLayer::new(64 * 1024))
        .layer(cors)
        .with_state(state.clone());

    tokio::spawn(retention_worker(state.clone()));
    let address: SocketAddr = config.bind.parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    info!("The Polls API listening on {}", address);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn connect_db(url: &str) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(8));
    SqlitePool::connect_with(options).await
}

async fn seed_elections(db: &SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
    #[derive(Deserialize)]
    struct Seed {
        elections: Vec<SeedElection>,
    }
    #[derive(Deserialize)]
    struct SeedElection {
        slug: String,
        title: String,
        region: String,
        election_date: String,
        open_at: String,
        close_at: String,
        included: bool,
        roster_status: String,
        source_url: String,
        source_label: String,
        candidates: Vec<SeedCandidate>,
    }
    #[derive(Deserialize)]
    struct SeedCandidate {
        party: String,
        name: String,
        running_mate: Option<String>,
    }
    let seed: Seed = serde_json::from_str(include_str!("../../data/seed/elections.json"))?;
    for election in seed.elections {
        sqlx::query("INSERT INTO elections(slug,title,region,election_date,open_at,close_at,included,roster_status,source_url,source_label) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(slug) DO UPDATE SET title=excluded.title,region=excluded.region,election_date=excluded.election_date,open_at=excluded.open_at,close_at=excluded.close_at,included=excluded.included,roster_status=excluded.roster_status,source_url=excluded.source_url,source_label=excluded.source_label")
            .bind(&election.slug).bind(&election.title).bind(&election.region)
            .bind(&election.election_date).bind(&election.open_at).bind(&election.close_at)
            .bind(election.included).bind(&election.roster_status).bind(&election.source_url)
            .bind(&election.source_label).execute(db).await?;
        let election_id: i64 = sqlx::query_scalar("SELECT id FROM elections WHERE slug=?")
            .bind(&election.slug)
            .fetch_one(db)
            .await?;
        for candidate in election.candidates {
            sqlx::query("INSERT INTO candidates(election_id,party,name,running_mate) VALUES(?,?,?,?) ON CONFLICT(election_id,party) DO UPDATE SET name=excluded.name,running_mate=excluded.running_mate")
                .bind(election_id).bind(candidate.party).bind(candidate.name).bind(candidate.running_mate)
                .execute(db).await?;
        }
    }
    Ok(())
}

async fn seed_admin(db: &SqlitePool, config: &Config) -> Result<(), sqlx::Error> {
    if let Some(hash) = &config.admin_password_hash {
        sqlx::query("INSERT INTO admins(username,password_hash) VALUES(?,?) ON CONFLICT(username) DO UPDATE SET password_hash=excluded.password_hash,active=1")
            .bind(&config.admin_username).bind(hash).execute(db).await?;
    }
    Ok(())
}

async fn health(State(state): State<AppState>) -> Response {
    let database_ok = sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&state.db)
        .await
        .is_ok();
    if !database_ok {
        return ApiError::response(StatusCode::SERVICE_UNAVAILABLE, "Database unavailable");
    }
    let stuck_payments: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ballots WHERE status IN ('pending','verifying') AND datetime(created_at)<=datetime('now','-15 minutes')")
        .fetch_one(&state.db).await.unwrap_or(0);
    let attention_refunds: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ballots WHERE status='refund_failed'")
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);
    // Operational warnings must not pull a healthy instance out of rotation.
    // `/api/health/ready` is the load-balancer probe; this endpoint is for
    // alerting.
    let degraded = stuck_payments > 0 || attention_refunds > 0;
    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "degraded": degraded,
            "service": "the-polls-api",
            "mode": state.config.mode_label(),
            "payments_ready": state.config.payments_ready(),
            "stuck_payments": stuck_payments,
            "refunds_needing_attention": attention_refunds
        })),
    )
        .into_response()
}

async fn health_ready(State(state): State<AppState>) -> Response {
    match sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&state.db)
        .await
    {
        Ok(_) => (StatusCode::OK, Json(json!({"ready": true}))).into_response(),
        Err(_) => ApiError::response(StatusCode::SERVICE_UNAVAILABLE, "Database unavailable"),
    }
}

async fn public_config(State(state): State<AppState>) -> Response {
    let mut response = Json(json!({
        "terms_version": TERMS_VERSION,
        "base_amount_kobo": BASE_AMOUNT_KOBO,
        "base_amount_ngn": 100,
        "demo_mode": state.config.demo_mode,
        "mode": state.config.mode_label(),
        "payments_ready": state.config.payments_ready(),
        "server_now": Utc::now().to_rfc3339()
    }))
    .into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, max-age=0"),
    );
    response
}

async fn terms() -> Json<Value> {
    Json(json!({
        "version": TERMS_VERSION,
        "base_amount_kobo": BASE_AMOUNT_KOBO,
        "base_amount_ngn": 100,
        "fee_notice": "The ballot costs ₦100. Paystack adds its processing fee at checkout; the final amount is shown by Paystack before you pay.",
        "summary": "This is a paid mock poll, not an official election or scientific poll. No prizes or accuracy claims are made. A successful verified payment is required for one ballot. Card authorization signatures are checked for repeat cards where Paystack supplies one; other controls are soft abuse signals and cannot guarantee one person, one ballot. Late, duplicate, mismatched, or uncountable successful payments are submitted for a full refund. Paystack determines when a refund is processed."
    }))
}

async fn list_elections(State(state): State<AppState>) -> Response {
    let elections = match sqlx::query_as::<_, ElectionRow>(
        "SELECT * FROM elections ORDER BY election_date, title",
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => rows,
        Err(_) => {
            return ApiError::response(StatusCode::INTERNAL_SERVER_ERROR, "Could not load polls");
        }
    };
    let mut out = Vec::new();
    for row in elections {
        let candidates = match load_candidates(&state.db, row.id).await {
            Ok(c) => c,
            Err(_) => {
                return ApiError::response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Could not load poll candidates",
                );
            }
        };
        out.push(public_election(&state, row, candidates));
    }
    Json(out).into_response()
}

async fn get_election(State(state): State<AppState>, Path(slug): Path<String>) -> Response {
    let row = match sqlx::query_as::<_, ElectionRow>("SELECT * FROM elections WHERE slug=?")
        .bind(slug)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return ApiError::response(StatusCode::NOT_FOUND, "Poll not found"),
        Err(_) => {
            return ApiError::response(StatusCode::INTERNAL_SERVER_ERROR, "Could not load poll");
        }
    };
    let candidates = match load_candidates(&state.db, row.id).await {
        Ok(c) => c,
        Err(_) => {
            return ApiError::response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not load poll candidates",
            );
        }
    };
    Json(public_election(&state, row, candidates)).into_response()
}

async fn get_tally(State(state): State<AppState>, Path(slug): Path<String>) -> Response {
    let election = match sqlx::query_as::<_, ElectionRow>("SELECT * FROM elections WHERE slug=?")
        .bind(slug)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return ApiError::response(StatusCode::NOT_FOUND, "Poll not found"),
        Err(_) => {
            return ApiError::response(StatusCode::INTERNAL_SERVER_ERROR, "Could not load tally");
        }
    };
    let mut results = match sqlx::query_as::<_, (String, String, Option<String>, i64)>(
        "SELECT c.party,c.name,c.running_mate,COUNT(b.id) FROM candidates c LEFT JOIN ballots b ON b.candidate_id=c.id AND b.status='counted' WHERE c.election_id=? GROUP BY c.id ORDER BY COUNT(b.id) DESC,c.party")
        .bind(election.id).fetch_all(&state.db).await {
        Ok(rows) => rows.into_iter().map(|(party,name,running_mate,total)| TallyCandidate {party,name,running_mate,total}).collect::<Vec<_>>(),
        Err(_) => return ApiError::response(StatusCode::INTERNAL_SERVER_ERROR, "Could not load tally"),
    };
    let archive = sqlx::query_as::<_, (String, String, i64)>(
            "SELECT party,candidate_label,total FROM tally_archives WHERE election_id=? ORDER BY total DESC,party")
            .bind(election.id).fetch_all(&state.db).await;
    if let Ok(rows) = archive
        && !rows.is_empty()
    {
        results = rows
            .into_iter()
            .map(|(party, name, total)| TallyCandidate {
                party,
                name,
                running_mate: None,
                total,
            })
            .collect();
    }
    let total_counted = results.iter().map(|r| r.total).sum();
    Json(TallyResponse {
        election: election.title,
        total_counted,
        updated_at: Utc::now().to_rfc3339(),
        results,
    })
    .into_response()
}

async fn load_candidates(db: &SqlitePool, id: i64) -> Result<Vec<CandidateRow>, sqlx::Error> {
    sqlx::query_as::<_, CandidateRow>(
        "SELECT id,party,name,running_mate FROM candidates WHERE election_id=? ORDER BY party",
    )
    .bind(id)
    .fetch_all(db)
    .await
}

fn public_election(
    state: &AppState,
    row: ElectionRow,
    candidates: Vec<CandidateRow>,
) -> ElectionPublic {
    let now = Utc::now();
    let demo = state.config.demo_mode;
    let open = demo || parse_date(&row.open_at).map(|d| now >= d).unwrap_or(false);
    let not_closed = demo
        || (parse_date(&row.close_at).map(|d| now <= d).unwrap_or(false)
            && row.force_closed_at.is_none());
    // Demo mode (test keys only) deliberately ignores roster and schedule so the
    // full payment loop can be rehearsed before launch. Live mode does not.
    let roster_ready = demo || row.roster_status == "ready";
    ElectionPublic {
        slug: row.slug,
        title: row.title,
        region: row.region,
        election_date: row.election_date,
        open_at: row.open_at,
        close_at: row.close_at,
        server_now: now.to_rfc3339(),
        included: row.included != 0,
        roster_status: row.roster_status,
        source_url: row.source_url,
        source_label: row.source_label,
        is_open: state.config.payments_ready()
            && row.included != 0
            && roster_ready
            && open
            && not_closed,
        demo_mode: demo,
        candidates,
    }
}

async fn init_vote(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<VoteInitRequest>,
) -> Response {
    if !state.limits.allow(
        &format!("vote-init:{}", client_key(&headers)),
        12,
        Duration::from_secs(300),
    ) {
        return rate_limited("Too many checkout attempts. Wait a few minutes and try again.");
    }
    if !state.config.payments_ready() {
        return ApiError::response(
            StatusCode::SERVICE_UNAVAILABLE,
            if state.config.demo_mode {
                "Test-mode checkout is not configured. Set a Paystack test secret, HMAC keys, and PAID_POLLING_ENABLED=1."
            } else {
                "Paid polling is not enabled. This poll is still in preview."
            },
        );
    }
    let secret = match state.config.paystack_secret.as_ref() {
        Some(s) => s,
        None => {
            return ApiError::response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Checkout is not configured",
            );
        }
    };
    if !input.accepted_terms || input.terms_version != TERMS_VERSION {
        return ApiError::response(
            StatusCode::BAD_REQUEST,
            "Accept the current terms before continuing",
        );
    }
    if input.channel != "card" && input.channel != "transfer" {
        return ApiError::response(StatusCode::BAD_REQUEST, "Choose card or Pay with Transfer");
    }
    if let Err(msg) = validate_location(input.country_code.as_deref(), input.state_code.as_deref())
    {
        return ApiError::response(StatusCode::BAD_REQUEST, msg);
    }
    let election = match sqlx::query_as::<_, ElectionRow>("SELECT * FROM elections WHERE slug=?")
        .bind(&input.election)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(e)) => e,
        Ok(None) => return ApiError::response(StatusCode::NOT_FOUND, "Poll not found"),
        Err(_) => {
            return ApiError::response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not validate poll",
            );
        }
    };
    if !public_election(&state, election.clone(), vec![]).is_open {
        return ApiError::response(StatusCode::CONFLICT, "This poll is not accepting ballots");
    }
    let candidate: Option<i64> =
        sqlx::query_scalar("SELECT id FROM candidates WHERE election_id=? AND party=?")
            .bind(election.id)
            .bind(input.candidate_party.trim().to_uppercase())
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);
    let candidate_id = match candidate {
        Some(id) => id,
        None => return ApiError::response(StatusCode::BAD_REQUEST, "Choose a listed candidate"),
    };

    let receipt_id = format!("pr_{}", hex::encode(random::<[u8; 16]>()));
    let reference = format!("poll_{}", hex::encode(random::<[u8; 16]>()));
    let now = Utc::now();
    let created_at = now.to_rfc3339();
    let expires_at = (now + ChronoDuration::hours(1)).to_rfc3339();
    let location = match (input.country_code.as_deref(), input.state_code.as_deref()) {
        (Some(country), Some(state)) => Some(format!("{}:{}", country, state)),
        (Some(country), None) => Some(country.to_owned()),
        _ => None,
    };
    let device_fingerprint = headers
        .get("x-device-token")
        .and_then(|v| v.to_str().ok())
        .filter(|v| v.len() >= 16 && v.len() <= 128)
        .and_then(|v| {
            state
                .config
                .device_hmac_key
                .as_ref()
                .map(|key| hmac256(key, &format!("election:{}:{}", election.id, v)))
        });
    let (requested, estimated_fee) = if input.channel == "transfer" {
        let gross = transfer_gross_up(BASE_AMOUNT_KOBO);
        (gross, gross - BASE_AMOUNT_KOBO)
    } else {
        (BASE_AMOUNT_KOBO, 0)
    };
    let transfer_expiry = if input.channel == "transfer" {
        Some(expires_at.clone())
    } else {
        None
    };

    let mut tx = match state.db.begin().await {
        Ok(tx) => tx,
        Err(_) => {
            return ApiError::response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not start checkout",
            );
        }
    };
    let insert_ballot = sqlx::query("INSERT INTO ballots(receipt_id,election_id,candidate_id,location,status,accepted_terms_version,accepted_terms_at,device_fingerprint,created_at) VALUES(?,?,?,?,'pending',?,?,?,?)")
        .bind(&receipt_id).bind(election.id).bind(candidate_id).bind(location).bind(TERMS_VERSION)
        .bind(&created_at).bind(device_fingerprint).bind(&created_at).execute(&mut *tx).await;
    if insert_ballot.is_err() {
        let _ = tx.rollback().await;
        return ApiError::response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not create receipt",
        );
    }
    let ballot_id = insert_ballot.unwrap().last_insert_rowid();
    let payinsert = sqlx::query("INSERT INTO payments(ballot_id,reference,base_amount_kobo,requested_amount_kobo,channel,transfer_expires_at,support_purge_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)")
        .bind(ballot_id).bind(&reference).bind(BASE_AMOUNT_KOBO).bind(requested).bind(&input.channel)
        .bind(transfer_expiry).bind((now + ChronoDuration::days(365)).to_rfc3339()).bind(&created_at).bind(&created_at).execute(&mut *tx).await;
    if payinsert.is_err() || tx.commit().await.is_err() {
        return ApiError::response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not create checkout",
        );
    }

    let email = format!("{}@{}", reference, state.config.synthetic_domain);
    let api = if input.channel == "card" {
        let payload = json!({
            "email": email,
            "amount": BASE_AMOUNT_KOBO.to_string(),
            "reference": reference,
            "currency": "NGN",
            "channels": ["card"],
            "callback_url": format!("{}/api/paystack/callback", state.config.api_public_url.trim_end_matches('/'))
        });
        state
            .http
            .post(format!(
                "{}/transaction/initialize",
                state.config.paystack_api_base.trim_end_matches('/')
            ))
            .bearer_auth(secret)
            .json(&payload)
            .send()
            .await
    } else {
        let payload = json!({
            "email": email,
            "amount": requested.to_string(),
            "reference": reference,
            "currency": "NGN",
            "bank_transfer": { "account_expires_at": expires_at }
        });
        state
            .http
            .post(format!(
                "{}/charge",
                state.config.paystack_api_base.trim_end_matches('/')
            ))
            .bearer_auth(secret)
            .json(&payload)
            .send()
            .await
    };
    let envelope = match api {
        Ok(response) if response.status().is_success() => {
            match response.json::<PaystackEnvelope<InitData>>().await {
                Ok(e) if e.status => e,
                _ => {
                    mark_abandoned(&state.db, ballot_id).await;
                    return ApiError::response(
                        StatusCode::BAD_GATEWAY,
                        "Paystack could not start this payment. No ballot was counted.",
                    );
                }
            }
        }
        _ => {
            mark_abandoned(&state.db, ballot_id).await;
            return ApiError::response(
                StatusCode::BAD_GATEWAY,
                "Paystack could not start this payment. No ballot was counted.",
            );
        }
    };
    let data = match envelope.data {
        Some(data) if data.reference.as_deref() == Some(reference.as_str()) => data,
        _ => {
            mark_abandoned(&state.db, ballot_id).await;
            return ApiError::response(
                StatusCode::BAD_GATEWAY,
                "Payment provider returned an invalid checkout response",
            );
        }
    };
    let response = if input.channel == "card" {
        let url = data
            .authorization_url
            .filter(|url| url.starts_with("https://checkout.paystack.com/"));
        let url = match url {
            Some(url) => url,
            None => {
                mark_abandoned(&state.db, ballot_id).await;
                return ApiError::response(
                    StatusCode::BAD_GATEWAY,
                    "Payment provider returned an invalid checkout URL",
                );
            }
        };
        VoteInitResponse {
            receipt_id,
            channel: input.channel,
            base_amount_kobo: BASE_AMOUNT_KOBO,
            requested_amount_kobo: requested,
            estimated_fee_kobo: 0,
            checkout_url: Some(url),
            transfer: None,
        }
    } else {
        let bank = data
            .bank
            .as_ref()
            .and_then(|v| v.get("name").or_else(|| v.get("bank_name")))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let provider_expiry = data.expires_at.or(data.account_expires_at).or_else(|| {
            data.bank
                .as_ref()
                .and_then(|v| v.get("account_expires_at"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
        let requested_date = parse_date(&expires_at);
        let provider_date = provider_expiry.as_deref().and_then(parse_date);
        let effective_expiry = match (requested_date, provider_date) {
            (Some(requested), Some(provider)) => Some(requested.min(provider).to_rfc3339()),
            (Some(requested), None) => Some(requested.to_rfc3339()),
            (None, Some(provider)) => Some(provider.to_rfc3339()),
            _ => None,
        };
        if let Some(expiry) = &effective_expiry {
            let _ = sqlx::query("UPDATE payments SET transfer_expires_at=? WHERE reference=?")
                .bind(expiry)
                .bind(&reference)
                .execute(&state.db)
                .await;
        }
        VoteInitResponse {
            receipt_id,
            channel: input.channel,
            base_amount_kobo: BASE_AMOUNT_KOBO,
            requested_amount_kobo: requested,
            estimated_fee_kobo: estimated_fee,
            checkout_url: None,
            transfer: Some(TransferInstructions {
                bank,
                account_name: data.account_name,
                account_number: data.account_number,
                expires_at: effective_expiry,
                message: data.display_text.unwrap_or_else(|| {
                    "Transfer the exact amount to this temporary account before it expires.".into()
                }),
            }),
        }
    };
    Json(response).into_response()
}

async fn mark_abandoned(db: &SqlitePool, ballot_id: i64) {
    let _ = sqlx::query("UPDATE ballots SET status='abandoned' WHERE id=? AND status='pending'")
        .bind(ballot_id)
        .execute(db)
        .await;
}

async fn get_receipt(State(state): State<AppState>, Path(receipt_id): Path<String>) -> Response {
    let lookup = normalize_public_ref(&receipt_id);
    let Some(lookup) = lookup else {
        return ApiError::response(StatusCode::NOT_FOUND, "Receipt not found");
    };
    let result = sqlx::query_as::<_, ReceiptRow>("SELECT b.receipt_id,b.vote_code,e.title,b.status,b.created_at,b.counted_at FROM ballots b JOIN elections e ON e.id=b.election_id WHERE b.receipt_id=? OR b.vote_code=?")
        .bind(&lookup).bind(&lookup).fetch_optional(&state.db).await;
    match result {
        Ok(Some(r)) => {
            let mut response = Json(ReceiptPublic {
                receipt_id: r.receipt_id,
                ballot_code: r.vote_code,
                poll: r.title,
                status: public_status(&r.status).to_owned(),
                created_at: r.created_at,
                counted_at: r.counted_at,
            })
            .into_response();
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-store, max-age=0"),
            );
            response
        }
        Ok(None) => ApiError::response(StatusCode::NOT_FOUND, "Receipt not found"),
        Err(_) => ApiError::response(StatusCode::INTERNAL_SERVER_ERROR, "Could not load receipt"),
    }
}

async fn paystack_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(key) = state.config.paystack_secret.as_ref() else {
        return ApiError::response(
            StatusCode::SERVICE_UNAVAILABLE,
            "Webhook verification is not configured",
        );
    };
    let Some(signature) = headers
        .get("x-paystack-signature")
        .and_then(|v| v.to_str().ok())
    else {
        return ApiError::response(StatusCode::UNAUTHORIZED, "Invalid webhook signature");
    };
    if !verify_hmac512(key, &body, signature) {
        return ApiError::response(StatusCode::UNAUTHORIZED, "Invalid webhook signature");
    }
    let value: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return ApiError::response(StatusCode::BAD_REQUEST, "Invalid webhook body"),
    };
    let event_type = value
        .get("event")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if event_type.starts_with("refund.") {
        return handle_refund_webhook(&state, &value, event_type).await;
    }
    if event_type == "bank.transfer.rejected" {
        let reference = value
            .pointer("/data/reference")
            .and_then(Value::as_str)
            .unwrap_or("");
        let event_source = format!(
            "{}:{}:{}",
            event_type,
            reference,
            value
                .pointer("/data/id")
                .map(Value::to_string)
                .unwrap_or_default()
        );
        let event_key = hex::encode(Sha256::digest(event_source.as_bytes()));
        let mut tx = match state.db.begin().await {
            Ok(tx) => tx,
            Err(_) => {
                return ApiError::response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Payment event could not be processed",
                );
            }
        };
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO webhook_events(event_key,event_type,received_at) VALUES(?,?,?)",
        )
        .bind(event_key)
        .bind(event_type)
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *tx)
        .await;
        match inserted {
            Ok(result) if result.rows_affected() == 0 => {
                let _ = tx.commit().await;
                return (StatusCode::OK, "ok").into_response();
            }
            Err(_) => {
                return ApiError::response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Payment event could not be processed",
                );
            }
            _ => {}
        }
        if sqlx::query("UPDATE ballots SET status='abandoned' WHERE id=(SELECT ballot_id FROM payments WHERE reference=?) AND status='pending'")
            .bind(reference).execute(&mut *tx).await.is_err() || tx.commit().await.is_err() {
            return ApiError::response(StatusCode::INTERNAL_SERVER_ERROR, "Payment event could not be processed");
        }
        return (StatusCode::OK, "ok").into_response();
    }
    if event_type != "charge.success" {
        return (StatusCode::OK, "ignored").into_response();
    }
    let reference = value
        .pointer("/data/reference")
        .and_then(Value::as_str)
        .unwrap_or("");
    if reference.is_empty() {
        return ApiError::response(StatusCode::BAD_REQUEST, "Missing payment reference");
    }
    // Events can arrive for other transactions on the same Paystack account.
    // Acknowledge them so Paystack stops retrying; only known references settle.
    let known: Option<i64> = sqlx::query_scalar("SELECT 1 FROM payments WHERE reference=?")
        .bind(reference)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
    if known.is_none() {
        return (StatusCode::OK, "ignored").into_response();
    }
    let event_source = format!(
        "{}:{}:{}",
        event_type,
        reference,
        value
            .pointer("/data/id")
            .map(Value::to_string)
            .unwrap_or_default()
    );
    let event_key = hex::encode(Sha256::digest(event_source.as_bytes()));
    match settle_reference(&state, reference, Some((event_key, event_type))).await {
        Ok(_) => (StatusCode::OK, "ok").into_response(),
        Err(_) => ApiError::response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Payment event could not be processed",
        ),
    }
}

async fn paystack_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Response {
    if !state.limits.allow(
        &format!("callback:{}", client_key(&headers)),
        120,
        Duration::from_secs(60),
    ) {
        return rate_limited("Too many receipt checks. Wait a moment and retry.");
    }
    let Some(reference) = query.reference else {
        return Redirect::to(&format!(
            "{}/",
            state.config.public_base_url.trim_end_matches('/')
        ))
        .into_response();
    };
    let receipt = match sqlx::query_scalar::<_, String>("SELECT b.receipt_id FROM payments p JOIN ballots b ON b.id=p.ballot_id WHERE p.reference=?")
        .bind(&reference).fetch_optional(&state.db).await {
        Ok(Some(r)) => r,
        _ => return ApiError::response(StatusCode::NOT_FOUND, "Payment receipt not found"),
    };
    let _ = settle_reference(&state, &reference, None).await;
    Redirect::to(&format!(
        "{}/receipt/{}",
        state.config.public_base_url.trim_end_matches('/'),
        receipt
    ))
    .into_response()
}

async fn settle_reference(
    state: &AppState,
    reference: &str,
    event: Option<(String, &str)>,
) -> Result<SettlementResult, ()> {
    let secret = state.config.paystack_secret.as_ref().ok_or(())?;
    let row = sqlx::query_as::<_, SettlementRow>("SELECT b.id AS ballot_id,b.election_id,b.status,p.base_amount_kobo,p.requested_amount_kobo,p.channel FROM payments p JOIN ballots b ON b.id=p.ballot_id WHERE p.reference=?")
        .bind(reference).fetch_optional(&state.db).await.map_err(|_|())?.ok_or(())?;
    if is_terminal_status(&row.status) {
        return Ok(SettlementResult::NoChange);
    }
    // Take exclusive ownership of verification. Only one worker may verify a
    // payment at a time; a crashed worker's claim is reclaimable after two
    // minutes so a ballot cannot be stranded in `verifying`.
    let claim_at = Utc::now().to_rfc3339();
    let claimed = sqlx::query(
        "UPDATE ballots SET status='verifying',verify_claim_at=? WHERE id=? AND (status='pending' OR (status IN ('verifying','abandoned') AND (verify_claim_at IS NULL OR datetime(verify_claim_at)<=datetime('now','-2 minutes'))))",
    )
    .bind(&claim_at)
    .bind(row.ballot_id)
    .execute(&state.db)
    .await
    .map_err(|_| ())?;
    if claimed.rows_affected() == 0 {
        return Ok(SettlementResult::NoChange);
    }
    let response = state
        .http
        .get(format!(
            "{}/transaction/verify/{}",
            state.config.paystack_api_base.trim_end_matches('/'),
            reference
        ))
        .bearer_auth(secret)
        .send()
        .await
        .map_err(|_| ())?;
    if !response.status().is_success() {
        return Err(());
    }
    let envelope: PaystackEnvelope<VerifiedTransaction> = response.json().await.map_err(|_| ())?;
    if !envelope.status {
        return Err(());
    }
    let tx_data = envelope.data.ok_or(())?;
    if tx_data.reference != reference {
        return Err(());
    }
    let now = Utc::now().to_rfc3339();
    let paid_success = tx_data.status.eq_ignore_ascii_case("success");
    // `amount` is what the customer actually paid. Paystack's fee is deducted
    // from the merchant payout; adding it back here would let an underpayment
    // count as a full ballot.
    let actual_amount = tx_data.amount;
    let fee = tx_data.fees;
    let currency_ok = tx_data
        .currency
        .as_deref()
        .unwrap_or("")
        .eq_ignore_ascii_case("NGN");
    let channel_ok = matches!(
        (
            row.channel.as_str(),
            tx_data
                .channel
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .as_str(),
        ),
        ("card", "card") | ("transfer", "bank" | "bank_transfer")
    );
    let amount_ok =
        actual_amount >= row.base_amount_kobo && actual_amount >= row.requested_amount_kobo;
    let paid_at = tx_data.paid_at.as_deref().and_then(parse_date);
    let mut dbtx = state.db.begin().await.map_err(|_| ())?;
    if !paid_success {
        if matches!(
            tx_data.status.to_lowercase().as_str(),
            "failed" | "abandoned" | "reversed"
        ) {
            sqlx::query("UPDATE ballots SET status='abandoned',verify_claim_at=NULL WHERE id=? AND status='verifying' AND verify_claim_at=?")
                .bind(row.ballot_id)
                .bind(&claim_at)
                .execute(&mut *dbtx)
                .await
                .map_err(|_| ())?;
        }
        dbtx.commit().await.map_err(|_| ())?;
        return Ok(SettlementResult::NoChange);
    }
    if let Some((event_key, event_type)) = event {
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO webhook_events(event_key,event_type,received_at) VALUES(?,?,?)",
        )
        .bind(event_key)
        .bind(event_type)
        .bind(&now)
        .execute(&mut *dbtx)
        .await
        .map_err(|_| ())?;
        if inserted.rows_affected() == 0 {
            dbtx.commit().await.map_err(|_| ())?;
            return Ok(SettlementResult::DuplicateEvent);
        }
    }
    let current_status: String = sqlx::query_scalar("SELECT status FROM ballots WHERE id=?")
        .bind(row.ballot_id)
        .fetch_one(&mut *dbtx)
        .await
        .map_err(|_| ())?;
    if is_terminal_status(&current_status) {
        dbtx.commit().await.map_err(|_| ())?;
        return Ok(SettlementResult::NoChange);
    }
    let poll_times = sqlx::query_as::<_, (String,String,Option<String>,Option<String>)>("SELECT e.open_at,e.close_at,e.force_closed_at,p.transfer_expires_at FROM elections e JOIN payments p ON p.ballot_id=? WHERE e.id=?")
        .bind(row.ballot_id).bind(row.election_id).fetch_one(&mut *dbtx).await.map_err(|_|())?;
    let opening = parse_date(&poll_times.0).ok_or(())?;
    let cutoff = parse_date(&poll_times.1).ok_or(())?;
    let force_cutoff = poll_times.2.as_deref().and_then(parse_date);
    let transfer_cutoff = poll_times.3.as_deref().and_then(parse_date);
    let on_time = state.config.demo_mode
        || paid_at
            .map(|p| {
                p >= opening
                    && p <= cutoff
                    && force_cutoff.map(|f| p <= f).unwrap_or(true)
                    && transfer_cutoff.map(|f| p <= f).unwrap_or(true)
            })
            .unwrap_or(false);
    let mut card_hash = None;
    if row.channel == "card"
        && let Some(signature) = tx_data
            .authorization
            .as_ref()
            .and_then(|a| a.signature.as_ref())
    {
        let key = state.config.card_fingerprint_hmac_key.as_deref();
        card_hash = key.map(|k| hmac256(k, &format!("card:{}:{}", row.election_id, signature)));
    }
    let mut duplicate_card = false;
    if paid_success
        && on_time
        && currency_ok
        && amount_ok
        && let Some(fingerprint) = &card_hash
    {
        let inserted = sqlx::query("INSERT OR IGNORE INTO card_locks(election_id,card_fingerprint,ballot_id,created_at) VALUES(?,?,?,?)")
            .bind(row.election_id).bind(fingerprint).bind(row.ballot_id).bind(&now)
            .execute(&mut *dbtx).await.map_err(|_|())?;
        duplicate_card = inserted.rows_affected() == 0;
    }
    let should_refund = !on_time || !currency_ok || !channel_ok || !amount_ok || duplicate_card;
    // The public ballot code is derived from the verified Paystack transaction.
    // It is one-way (HMAC), election-scoped, and never reveals the candidate or
    // the private payment reference.
    let vote_code = state
        .config
        .card_fingerprint_hmac_key
        .as_deref()
        .map(|key| vote_code_for(key, row.election_id, &tx_data.id.to_string()));
    sqlx::query("UPDATE payments SET actual_amount_kobo=?,fee_kobo=?,provider_transaction_id=?,paid_at=?,authorization_hash=?,updated_at=? WHERE ballot_id=?")
        .bind(actual_amount).bind(fee).bind(tx_data.id).bind(tx_data.paid_at.clone()).bind(card_hash).bind(&now).bind(row.ballot_id)
        .execute(&mut *dbtx).await.map_err(|_|())?;
    let applied = if should_refund {
        sqlx::query("UPDATE ballots SET status='refund_pending',vote_code=?,verify_claim_at=NULL WHERE id=? AND status='verifying' AND verify_claim_at=?")
            .bind(vote_code.as_deref()).bind(row.ballot_id).bind(&claim_at)
            .execute(&mut *dbtx).await.map_err(|_|())?.rows_affected()
    } else {
        sqlx::query("UPDATE ballots SET status='counted',counted_at=?,vote_code=?,verify_claim_at=NULL WHERE id=? AND status='verifying' AND verify_claim_at=?")
            .bind(&now).bind(vote_code.as_deref()).bind(row.ballot_id).bind(&claim_at)
            .execute(&mut *dbtx).await.map_err(|_|())?.rows_affected()
    };
    // A zero-row final update means another worker reclaimed and settled the
    // ballot after our two-minute claim expired. Do not double-apply.
    if applied == 0 {
        dbtx.rollback().await.map_err(|_| ())?;
        return Ok(SettlementResult::NoChange);
    }
    dbtx.commit().await.map_err(|_| ())?;
    if should_refund {
        let actual = actual_amount.max(0);
        let _ = request_refund(state, row.ballot_id, actual).await;
        return Ok(SettlementResult::RefundRequired);
    }
    Ok(SettlementResult::Counted)
}

async fn request_refund(state: &AppState, ballot_id: i64, amount: i64) -> Result<(), ()> {
    let Some(secret) = state.config.paystack_secret.as_ref() else {
        return Err(());
    };
    let record = sqlx::query_as::<_, (Option<String>, Option<String>, Option<i64>, Option<String>, Option<String>)>("SELECT provider_transaction_id,reference,actual_amount_kobo,refund_reference,refund_status FROM payments WHERE ballot_id=?")
        .bind(ballot_id).fetch_optional(&state.db).await.map_err(|_|())?.ok_or(())?;
    if record.2.unwrap_or(0) <= 0 || amount <= 0 {
        mark_refund_attention(&state.db, ballot_id).await;
        return Err(());
    }
    if matches!(record.4.as_deref(), Some("processed" | "success")) {
        let _ = sqlx::query("UPDATE ballots SET status='refunded' WHERE id=?")
            .bind(ballot_id)
            .execute(&state.db)
            .await;
        return Ok(());
    }
    if matches!(
        record.4.as_deref(),
        Some("pending" | "processing" | "needs_attention")
    ) {
        return Ok(());
    }
    let fresh_reference = format!("rf_{}", hex::encode(random::<[u8; 16]>()));
    let idempotency_reference = if matches!(record.4.as_deref(), Some("failed" | "rejected")) {
        fresh_reference
    } else {
        record.3.clone().unwrap_or(fresh_reference)
    };
    let mut tx = state.db.begin().await.map_err(|_| ())?;
    sqlx::query("UPDATE ballots SET status='refunding' WHERE id=? AND status IN ('refund_pending','refund_failed')")
        .bind(ballot_id).execute(&mut *tx).await.map_err(|_|())?;
    sqlx::query("UPDATE payments SET refund_status='pending',refund_reference=?,updated_at=? WHERE ballot_id=?")
        .bind(&idempotency_reference).bind(Utc::now().to_rfc3339()).bind(ballot_id).execute(&mut *tx).await.map_err(|_|())?;
    tx.commit().await.map_err(|_| ())?;
    let identifier = record.0.or(record.1).ok_or(())?;
    // Omitting `amount` asks Paystack for the full transaction refund, and its `reference`
    // makes retries idempotent. Paystack's processing fee is not returned.
    let payload =
        json!({"transaction":identifier,"currency":"NGN","reference":idempotency_reference});
    let response = match state
        .http
        .post(format!(
            "{}/refund",
            state.config.paystack_api_base.trim_end_matches('/')
        ))
        .bearer_auth(secret)
        .json(&payload)
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => {
            mark_refund_attention(&state.db, ballot_id).await;
            return Err(());
        }
    };
    let body: Value = match response.json().await {
        Ok(body) => body,
        Err(_) => {
            mark_refund_attention(&state.db, ballot_id).await;
            return Err(());
        }
    };
    let status = body
        .pointer("/data/status")
        .and_then(Value::as_str)
        .unwrap_or("pending");
    if body.get("status").and_then(Value::as_bool) == Some(true) {
        sqlx::query("UPDATE payments SET refund_status=?,updated_at=? WHERE ballot_id=?")
            .bind(status)
            .bind(Utc::now().to_rfc3339())
            .bind(ballot_id)
            .execute(&state.db)
            .await
            .map_err(|_| ())?;
        if status.eq_ignore_ascii_case("processed") || status.eq_ignore_ascii_case("success") {
            sqlx::query("UPDATE ballots SET status='refunded' WHERE id=? AND status='refunding'")
                .bind(ballot_id)
                .execute(&state.db)
                .await
                .map_err(|_| ())?;
        } else if status.eq_ignore_ascii_case("failed")
            || status.eq_ignore_ascii_case("rejected")
            || status.eq_ignore_ascii_case("needs-attention")
        {
            let normalized = if status.eq_ignore_ascii_case("needs-attention") {
                "needs_attention"
            } else {
                status
            };
            sqlx::query("UPDATE payments SET refund_status=? WHERE ballot_id=?")
                .bind(normalized)
                .bind(ballot_id)
                .execute(&state.db)
                .await
                .map_err(|_| ())?;
            sqlx::query(
                "UPDATE ballots SET status='refund_failed' WHERE id=? AND status='refunding'",
            )
            .bind(ballot_id)
            .execute(&state.db)
            .await
            .map_err(|_| ())?;
        }
    } else {
        mark_refund_attention(&state.db, ballot_id).await;
        return Err(());
    }
    Ok(())
}

async fn mark_refund_attention(db: &SqlitePool, ballot_id: i64) {
    let now = Utc::now().to_rfc3339();
    let _ = sqlx::query("UPDATE payments SET refund_status='needs_attention',refund_last_checked_at=?,updated_at=? WHERE ballot_id=?")
        .bind(&now).bind(&now).bind(ballot_id).execute(db).await;
    let _ = sqlx::query("UPDATE ballots SET status='refund_failed' WHERE id=? AND status IN ('refund_pending','refunding')")
        .bind(ballot_id).execute(db).await;
}

async fn handle_refund_webhook(state: &AppState, value: &Value, event_type: &str) -> Response {
    let reference = value
        .pointer("/data/transaction/reference")
        .and_then(Value::as_str)
        .or_else(|| value.pointer("/data/reference").and_then(Value::as_str))
        .unwrap_or("");
    let refund_status = value
        .pointer("/data/status")
        .and_then(Value::as_str)
        .unwrap_or("");
    let event_source = format!(
        "{}:{}:{}",
        event_type,
        reference,
        value
            .pointer("/data/id")
            .map(Value::to_string)
            .unwrap_or_default()
    );
    let event_key = hex::encode(Sha256::digest(event_source.as_bytes()));
    let mut tx = match state.db.begin().await {
        Ok(tx) => tx,
        Err(_) => {
            return ApiError::response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Refund event could not be processed",
            );
        }
    };
    let inserted = sqlx::query(
        "INSERT OR IGNORE INTO webhook_events(event_key,event_type,received_at) VALUES(?,?,?)",
    )
    .bind(event_key)
    .bind(event_type)
    .bind(Utc::now().to_rfc3339())
    .execute(&mut *tx)
    .await;
    match inserted {
        Ok(result) if result.rows_affected() == 0 => {
            let _ = tx.commit().await;
            return (StatusCode::OK, "ok").into_response();
        }
        Err(_) => {
            return ApiError::response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Refund event could not be processed",
            );
        }
        _ => {}
    }
    let refund_reference = value
        .pointer("/data/reference")
        .and_then(Value::as_str)
        .unwrap_or("");
    let provider_id = value
        .pointer("/data/transaction/id")
        .map(|id| id.to_string());
    let ballot_id = sqlx::query_scalar::<_, i64>("SELECT ballot_id FROM payments WHERE refund_reference=? OR reference=? OR provider_transaction_id=? LIMIT 1")
        .bind(refund_reference).bind(reference).bind(provider_id).fetch_optional(&mut *tx).await.unwrap_or(None);
    if let Some(ballot_id) = ballot_id {
        let processed =
            event_type == "refund.processed" || refund_status.eq_ignore_ascii_case("processed");
        let failed = event_type == "refund.failed"
            || refund_status.eq_ignore_ascii_case("failed")
            || refund_status.eq_ignore_ascii_case("rejected");
        let status = if processed {
            "processed"
        } else if failed {
            "failed"
        } else if refund_status.eq_ignore_ascii_case("needs-attention") {
            "needs_attention"
        } else {
            refund_status
        };
        let _ = sqlx::query("UPDATE payments SET refund_status=?,refund_last_checked_at=?,updated_at=? WHERE ballot_id=?")
            .bind(status).bind(Utc::now().to_rfc3339()).bind(Utc::now().to_rfc3339()).bind(ballot_id).execute(&mut *tx).await;
        if processed {
            let _ = sqlx::query("UPDATE ballots SET status='refunded' WHERE id=? AND status IN ('refunding','refund_pending','refund_failed')").bind(ballot_id).execute(&mut *tx).await;
        } else if failed || status == "needs_attention" {
            let _ = sqlx::query("UPDATE ballots SET status='refund_failed' WHERE id=?")
                .bind(ballot_id)
                .execute(&mut *tx)
                .await;
        }
    }
    if tx.commit().await.is_err() {
        return ApiError::response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Refund event could not be processed",
        );
    }
    (StatusCode::OK, "ok").into_response()
}

async fn admin_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Response {
    if !state.limits.allow(
        &format!("admin-login:{}", client_key(&headers)),
        5,
        Duration::from_secs(600),
    ) {
        return rate_limited("Too many sign-in attempts. Try again in ten minutes.");
    }
    let hash = match state.config.admin_password_hash.as_ref() {
        Some(h) if req.username == state.config.admin_username => h,
        _ => return ApiError::response(StatusCode::UNAUTHORIZED, "Invalid credentials"),
    };
    let parsed_hash = match argon2::PasswordHash::new(hash) {
        Ok(p) => p,
        Err(_) => {
            return ApiError::response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Admin authentication is not configured",
            );
        }
    };
    let valid = argon2::Argon2::default()
        .verify_password(req.password.as_bytes(), &parsed_hash)
        .is_ok();
    if !valid {
        return ApiError::response(StatusCode::UNAUTHORIZED, "Invalid credentials");
    }
    let Some(key) = state.config.admin_session_hmac_key.as_ref() else {
        return ApiError::response(
            StatusCode::SERVICE_UNAVAILABLE,
            "Admin sessions are not configured",
        );
    };
    let token = hex::encode(random::<[u8; 32]>());
    let csrf = hex::encode(random::<[u8; 32]>());
    let expires_at = (Utc::now() + ChronoDuration::hours(8)).to_rfc3339();
    let token_hash = hmac256(key, &token);
    let csrf_hash = hmac256(key, &csrf);
    let admin_id =
        sqlx::query_scalar::<_, i64>("SELECT id FROM admins WHERE username=? AND active=1")
            .bind(&req.username)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);
    let Some(admin_id) = admin_id else {
        return ApiError::response(StatusCode::UNAUTHORIZED, "Invalid credentials");
    };
    if sqlx::query("INSERT INTO admin_sessions(token_hash,admin_id,csrf_hash,expires_at,created_at) VALUES(?,?,?,?,?)")
        .bind(token_hash).bind(admin_id).bind(csrf_hash).bind(expires_at).bind(Utc::now().to_rfc3339()).execute(&state.db).await.is_err() {
        return ApiError::response(StatusCode::INTERNAL_SERVER_ERROR, "Could not create admin session");
    }
    let secure = if state.config.cookie_secure {
        "; Secure"
    } else {
        ""
    };
    let cookie = format!(
        "polls_admin={}; Path=/api/admin; HttpOnly; SameSite=Strict; Max-Age=28800{}",
        token, secure
    );
    let mut response = Json(json!({"ok":true,"csrf_token":csrf})).into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());
    response
}

async fn admin_logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(session) = admin_session(&state, &headers, true).await else {
        return ApiError::response(StatusCode::UNAUTHORIZED, "Admin session required");
    };
    let _ = sqlx::query("DELETE FROM admin_sessions WHERE token_hash=?")
        .bind(session.0)
        .execute(&state.db)
        .await;
    let mut response = Json(json!({"ok":true})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static(
            "polls_admin=; Path=/api/admin; HttpOnly; SameSite=Strict; Max-Age=0",
        ),
    );
    response
}

async fn admin_refunds(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if admin_session(&state, &headers, false).await.is_none() {
        return ApiError::response(StatusCode::UNAUTHORIZED, "Admin session required");
    }
    let rows = sqlx::query_as::<_, RefundQueueRow>("SELECT b.receipt_id,e.title AS election,b.status,p.actual_amount_kobo,p.refund_status,p.refund_reference FROM ballots b JOIN elections e ON e.id=b.election_id LEFT JOIN payments p ON p.ballot_id=b.id WHERE b.status IN ('refund_pending','refunding','refund_failed') OR p.refund_status IN ('pending','needs_attention','failed') ORDER BY b.created_at")
        .fetch_all(&state.db).await.unwrap_or_default();
    Json(json!({"refunds":rows})).into_response()
}

async fn admin_overview(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if admin_session(&state, &headers, false).await.is_none() {
        return ApiError::response(StatusCode::UNAUTHORIZED, "Admin session required");
    }
    let rows = sqlx::query_as::<_, AdminElectionRow>(
        "SELECT e.slug,e.title,e.open_at,e.close_at,e.included,e.roster_status,e.force_closed_at,
            (SELECT COUNT(*) FROM ballots b WHERE b.election_id=e.id AND b.status='counted') AS counted,
            (SELECT COUNT(*) FROM ballots b WHERE b.election_id=e.id AND b.status IN ('pending','verifying')) AS awaiting_payment,
            (SELECT COUNT(*) FROM ballots b WHERE b.election_id=e.id AND b.status IN ('refund_pending','refunding','refund_failed')) AS refunds
         FROM elections e ORDER BY e.election_date,e.title",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    Json(json!({
        "mode": state.config.mode_label(),
        "demo_mode": state.config.demo_mode,
        "payments_ready": state.config.payments_ready(),
        "launch_gates_complete": state.config.launch_gates_complete(),
        "terms_version": TERMS_VERSION,
        "elections": rows
    }))
    .into_response()
}

async fn admin_risk_signals(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if admin_session(&state, &headers, false).await.is_none() {
        return ApiError::response(StatusCode::UNAUTHORIZED, "Admin session required");
    }
    let rows = sqlx::query_as::<_, (String, i64, i64)>(
        "SELECT e.title,COUNT(DISTINCT b.device_fingerprint),COUNT(b.id) FROM ballots b JOIN elections e ON e.id=b.election_id WHERE b.device_fingerprint IS NOT NULL AND EXISTS (SELECT 1 FROM ballots other WHERE other.election_id=b.election_id AND other.device_fingerprint=b.device_fingerprint AND other.id<>b.id) GROUP BY e.id ORDER BY e.title"
    ).fetch_all(&state.db).await.unwrap_or_default();
    let signals: Vec<Value> = rows.into_iter().map(|(poll,repeated_groups,ballots)| json!({"poll":poll,"repeated_device_groups":repeated_groups,"ballots_with_repeats":ballots})).collect();
    Json(json!({"signals":signals,"notice":"These election-scoped HMAC device-token repeats are soft abuse signals only. They do not identify a person and never block a ballot."})).into_response()
}

async fn admin_lookup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LookupRequest>,
) -> Response {
    let Some(session) = admin_session(&state, &headers, true).await else {
        return ApiError::response(StatusCode::UNAUTHORIZED, "Admin session required");
    };
    let row = sqlx::query_as::<_, (String,String,String,Option<i64>,Option<i64>,Option<String>)>(
        "SELECT b.receipt_id,e.title,b.status,p.base_amount_kobo,p.actual_amount_kobo,p.refund_status FROM payments p JOIN ballots b ON b.id=p.ballot_id JOIN elections e ON e.id=b.election_id WHERE p.reference=?")
        .bind(req.reference.trim()).fetch_optional(&state.db).await.unwrap_or(None);
    if let Some(row) = row {
        audit(&state.db, session.1, "payment_lookup", &row.0, json!({})).await;
        Json(json!({"receipt_id":row.0,"poll":row.1,"status":row.2,"base_amount_kobo":row.3,"actual_amount_kobo":row.4,"refund_status":row.5})).into_response()
    } else {
        ApiError::response(StatusCode::NOT_FOUND, "Payment reference not found")
    }
}

async fn admin_refund(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RefundRequest>,
) -> Response {
    let Some(session) = admin_session(&state, &headers, true).await else {
        return ApiError::response(StatusCode::UNAUTHORIZED, "Admin session required");
    };
    let row = sqlx::query_as::<_, (i64,Option<i64>,String,Option<String>)>("SELECT b.id,p.actual_amount_kobo,b.status,p.refund_status FROM ballots b JOIN payments p ON p.ballot_id=b.id WHERE b.receipt_id=?")
        .bind(&req.receipt_id).fetch_optional(&state.db).await.unwrap_or(None);
    let Some((id, amount, status, refund_status)) = row else {
        return ApiError::response(StatusCode::NOT_FOUND, "Receipt not found");
    };
    if amount.unwrap_or(0) <= 0
        || !matches!(status.as_str(), "refund_pending" | "refund_failed")
        || !matches!(refund_status.as_deref(), Some("failed" | "rejected"))
    {
        return ApiError::response(StatusCode::CONFLICT, "This receipt has no retryable refund");
    }
    audit(
        &state.db,
        session.1,
        "refund_retry",
        &req.receipt_id,
        json!({}),
    )
    .await;
    match request_refund(&state, id, amount.unwrap()).await {
        Ok(()) => Json(json!({"ok":true,"message":"Refund request sent to Paystack; check again for its processed status."})).into_response(),
        Err(_) => ApiError::response(StatusCode::BAD_GATEWAY, "Paystack could not confirm the refund request. It needs attention."),
    }
}

async fn admin_refund_check(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RefundRequest>,
) -> Response {
    let Some(session) = admin_session(&state, &headers, true).await else {
        return ApiError::response(StatusCode::UNAUTHORIZED, "Admin session required");
    };
    let Some(secret) = state.config.paystack_secret.as_ref() else {
        return ApiError::response(
            StatusCode::SERVICE_UNAVAILABLE,
            "Payment provider is not configured",
        );
    };
    let row = sqlx::query_as::<_, (i64,Option<String>,String)>("SELECT b.id,p.refund_reference,b.status FROM ballots b JOIN payments p ON p.ballot_id=b.id WHERE b.receipt_id=?")
        .bind(&req.receipt_id).fetch_optional(&state.db).await.unwrap_or(None);
    let Some((ballot_id, Some(refund_reference), _)) = row else {
        return ApiError::response(StatusCode::NOT_FOUND, "Refund reference not found");
    };
    let url = format!(
        "{}/refund?reference={}",
        state.config.paystack_api_base.trim_end_matches('/'),
        refund_reference
    );
    let response = match state.http.get(url).bearer_auth(secret).send().await {
        Ok(r) => r,
        Err(_) => {
            return ApiError::response(
                StatusCode::BAD_GATEWAY,
                "Could not check refund status with Paystack",
            );
        }
    };
    let body: Value = match response.json().await {
        Ok(v) => v,
        Err(_) => {
            return ApiError::response(
                StatusCode::BAD_GATEWAY,
                "Could not read refund status from Paystack",
            );
        }
    };
    let data = body
        .get("data")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .cloned()
        .or_else(|| body.get("data").filter(|v| v.is_object()).cloned());
    let Some(data) = data else {
        return ApiError::response(
            StatusCode::NOT_FOUND,
            "Paystack has no refund record for this reference yet",
        );
    };
    let provider_status = data
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("pending");
    let normalized = match provider_status.to_lowercase().as_str() {
        "processed" | "success" => "processed",
        "failed" | "rejected" => "failed",
        "needs-attention" | "needs_attention" => "needs_attention",
        "processing" => "processing",
        _ => "pending",
    };
    let now = Utc::now().to_rfc3339();
    let _ = sqlx::query("UPDATE payments SET refund_status=?,refund_last_checked_at=?,updated_at=? WHERE ballot_id=?")
        .bind(normalized).bind(&now).bind(&now).bind(ballot_id).execute(&state.db).await;
    let ballot_status = if normalized == "processed" {
        "refunded"
    } else if normalized == "failed" || normalized == "needs_attention" {
        "refund_failed"
    } else {
        "refunding"
    };
    let _ = sqlx::query("UPDATE ballots SET status=? WHERE id=? AND status IN ('refund_pending','refunding','refund_failed')")
        .bind(ballot_status).bind(ballot_id).execute(&state.db).await;
    audit(
        &state.db,
        session.1,
        "refund_status_check",
        &req.receipt_id,
        json!({"status":normalized}),
    )
    .await;
    Json(json!({"receipt_id":req.receipt_id,"refund_status":normalized,"message":if normalized == "needs_attention" {"Paystack requires manual follow-up. No payer bank details are stored here."} else {"Status refreshed from Paystack."}})).into_response()
}

async fn admin_force_close(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CloseRequest>,
) -> Response {
    let Some(session) = admin_session(&state, &headers, true).await else {
        return ApiError::response(StatusCode::UNAUTHORIZED, "Admin session required");
    };
    let result = sqlx::query(
        "UPDATE elections SET force_closed_at=? WHERE slug=? AND force_closed_at IS NULL",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(&req.election)
    .execute(&state.db)
    .await;
    match result {
        Ok(r) if r.rows_affected() > 0 => {
            audit(
                &state.db,
                session.1,
                "force_close",
                &req.election,
                json!({}),
            )
            .await;
            Json(json!({"ok":true,"status":"closed"})).into_response()
        }
        Ok(_) => ApiError::response(StatusCode::NOT_FOUND, "Poll not found or already closed"),
        Err(_) => ApiError::response(StatusCode::INTERNAL_SERVER_ERROR, "Could not close poll"),
    }
}

async fn admin_session(
    state: &AppState,
    headers: &HeaderMap,
    require_csrf: bool,
) -> Option<(String, i64)> {
    let key = state.config.admin_session_hmac_key.as_ref()?;
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    let token = cookie
        .split(';')
        .find_map(|piece| piece.trim().strip_prefix("polls_admin="))?;
    let token_hash = hmac256(key, token);
    let row = sqlx::query_as::<_, (String,i64,String)>("SELECT token_hash,admin_id,csrf_hash FROM admin_sessions WHERE token_hash=? AND expires_at>? ")
        .bind(&token_hash).bind(Utc::now().to_rfc3339()).fetch_optional(&state.db).await.ok()??;
    if require_csrf {
        let csrf = headers.get("x-csrf-token")?.to_str().ok()?;
        let csrf_hash = hmac256(key, csrf);
        if !ct_eq(row.2.as_bytes(), csrf_hash.as_bytes()) {
            return None;
        }
    }
    Some((row.0, row.1))
}

async fn audit(db: &SqlitePool, admin_id: i64, action: &str, target: &str, detail: Value) {
    let _ = sqlx::query(
        "INSERT INTO admin_audit(admin_id,action,target,created_at,detail) VALUES(?,?,?,?,?)",
    )
    .bind(admin_id)
    .bind(action)
    .bind(target)
    .bind(Utc::now().to_rfc3339())
    .bind(detail.to_string())
    .execute(db)
    .await;
}

async fn retention_worker(state: AppState) {
    let mut tick = interval(Duration::from_secs(5 * 60));
    loop {
        tick.tick().await;
        if let Err(error) = run_reconciliation(&state).await {
            error!("payment reconciliation failed: {}", error);
        }
        if let Err(error) = run_retention(&state.db).await {
            error!("privacy retention job failed: {}", error);
        }
    }
}

async fn run_reconciliation(state: &AppState) -> Result<(), sqlx::Error> {
    let pending = sqlx::query_as::<_, (String,String,Option<String>)>("SELECT p.reference,b.status,p.transfer_expires_at FROM payments p JOIN ballots b ON b.id=p.ballot_id WHERE b.status IN ('pending','verifying') AND datetime(b.created_at)<=datetime('now','-5 minutes') ORDER BY b.created_at LIMIT 50")
        .fetch_all(&state.db).await?;
    for (reference, _, expiry) in pending {
        let _ = settle_reference(state, &reference, None).await;
        if expiry
            .as_deref()
            .and_then(parse_date)
            .is_some_and(|d| d <= Utc::now())
        {
            // Only abandon a ballot no worker is actively verifying. A stuck
            // `verifying` row is reclaimed by the two-minute claim timeout.
            let _ = sqlx::query("UPDATE ballots SET status='abandoned' WHERE id=(SELECT ballot_id FROM payments WHERE reference=?) AND status='pending'")
                .bind(&reference).execute(&state.db).await;
        }
    }
    let refunds = sqlx::query_as::<_, (i64,String)>("SELECT p.ballot_id,p.refund_reference FROM payments p JOIN ballots b ON b.id=p.ballot_id WHERE p.refund_reference IS NOT NULL AND p.refund_status IN ('pending','processing') AND datetime(COALESCE(p.refund_last_checked_at,p.updated_at))<=datetime('now','-10 minutes') ORDER BY p.updated_at LIMIT 50")
        .fetch_all(&state.db).await?;
    for (ballot_id, reference) in refunds {
        let _ = refresh_refund_status(state, ballot_id, &reference).await;
    }
    let stuck: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ballots WHERE status IN ('pending','verifying') AND datetime(created_at)<=datetime('now','-15 minutes')")
        .fetch_one(&state.db).await?;
    let attention: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ballots WHERE status='refund_failed'")
            .fetch_one(&state.db)
            .await?;
    if stuck > 0 || attention > 0 {
        error!(
            "operations alert: {} stuck payments, {} refunds need attention",
            stuck, attention
        );
    }
    Ok(())
}

async fn refresh_refund_status(
    state: &AppState,
    ballot_id: i64,
    reference: &str,
) -> Result<String, ()> {
    let Some(secret) = state.config.paystack_secret.as_ref() else {
        return Err(());
    };
    let url = format!(
        "{}/refund?reference={}",
        state.config.paystack_api_base.trim_end_matches('/'),
        reference
    );
    let response = state
        .http
        .get(url)
        .bearer_auth(secret)
        .send()
        .await
        .map_err(|_| ())?;
    let body: Value = response.json().await.map_err(|_| ())?;
    let data = body
        .get("data")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .cloned()
        .or_else(|| body.get("data").filter(|v| v.is_object()).cloned())
        .ok_or(())?;
    let provider_status = data
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("pending");
    let normalized = match provider_status.to_lowercase().as_str() {
        "processed" | "success" => "processed",
        "failed" | "rejected" => "failed",
        "needs-attention" | "needs_attention" => "needs_attention",
        "processing" => "processing",
        _ => "pending",
    };
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE payments SET refund_status=?,refund_last_checked_at=?,updated_at=? WHERE ballot_id=?")
        .bind(normalized).bind(&now).bind(&now).bind(ballot_id).execute(&state.db).await.map_err(|_|())?;
    let ballot_status = if normalized == "processed" {
        "refunded"
    } else if normalized == "failed" || normalized == "needs_attention" {
        "refund_failed"
    } else {
        "refunding"
    };
    sqlx::query("UPDATE ballots SET status=? WHERE id=? AND status IN ('refund_pending','refunding','refund_failed')")
        .bind(ballot_status).bind(ballot_id).execute(&state.db).await.map_err(|_|())?;
    Ok(normalized.into())
}

async fn run_retention(db: &SqlitePool) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    let cutoff60 = (Utc::now() - ChronoDuration::days(60)).to_rfc3339();
    let cutoff365 = (Utc::now() - ChronoDuration::days(365)).to_rfc3339();
    let mut tx = db.begin().await?;
    let elections = sqlx::query_as::<_, (i64,)>(
        "SELECT id FROM elections WHERE datetime(close_at)<=datetime(?)",
    )
    .bind(&cutoff60)
    .fetch_all(&mut *tx)
    .await?;
    let mut detached = 0_i64;
    for (election_id,) in elections {
        sqlx::query("INSERT INTO tally_archives(election_id,candidate_label,party,total,archived_at) SELECT e.id,c.name,c.party,COUNT(b.id),? FROM elections e JOIN candidates c ON c.election_id=e.id LEFT JOIN ballots b ON b.candidate_id=c.id AND b.status='counted' WHERE e.id=? GROUP BY c.id ON CONFLICT(election_id,party) DO UPDATE SET candidate_label=excluded.candidate_label,total=excluded.total,archived_at=excluded.archived_at")
            .bind(&now).bind(election_id).execute(&mut *tx).await?;
        let result = sqlx::query("UPDATE ballots SET candidate_id=NULL,location=NULL,detached_at=? WHERE election_id=? AND candidate_id IS NOT NULL")
            .bind(&now).bind(election_id).execute(&mut *tx).await?;
        detached += result.rows_affected() as i64;
    }
    let mut purged = 0_i64;
    let old_elections = sqlx::query_as::<_, (i64,)>(
        "SELECT id FROM elections WHERE datetime(close_at)<=datetime(?)",
    )
    .bind(&cutoff365)
    .fetch_all(&mut *tx)
    .await?;
    for (election_id,) in old_elections {
        let result = sqlx::query(
            "DELETE FROM payments WHERE ballot_id IN (SELECT id FROM ballots WHERE election_id=?)",
        )
        .bind(election_id)
        .execute(&mut *tx)
        .await?;
        purged += result.rows_affected() as i64;
        sqlx::query("DELETE FROM card_locks WHERE election_id=?")
            .bind(election_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE ballots SET card_fingerprint=NULL,device_fingerprint=NULL,accepted_terms_at=NULL,accepted_terms_version=NULL WHERE election_id=?")
            .bind(election_id).execute(&mut *tx).await?;
    }
    sqlx::query("DELETE FROM webhook_events WHERE datetime(received_at)<=datetime(?)")
        .bind(&cutoff365)
        .execute(&mut *tx)
        .await?;
    if detached > 0 || purged > 0 {
        sqlx::query(
            "INSERT INTO privacy_purge_log(detached_ballots,purged_payments,ran_at) VALUES(?,?,?)",
        )
        .bind(detached)
        .bind(purged)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

fn transfer_gross_up(base_kobo: i64) -> i64 {
    // NG local rate is 1.5%; the Paystack flat NGN 100 charge is waived below NGN 2,500.
    // One extra kobo follows Paystack's pass-fee formula to cover rounding.
    let numerator = base_kobo.saturating_mul(10_000);
    let denominator = 10_000 - 150;
    (numerator + denominator - 1) / denominator + 1
}

fn validate_location(country: Option<&str>, state: Option<&str>) -> Result<(), &'static str> {
    const STATES: &[&str] = &[
        "AB", "AD", "AK", "AN", "BA", "BY", "BE", "BO", "CR", "DE", "EB", "ED", "EK", "EN", "FC",
        "GO", "IM", "JI", "KD", "KN", "KT", "KE", "KO", "KW", "LA", "NA", "NI", "OG", "ON", "OS",
        "OY", "PL", "RI", "SO", "TA", "YO", "ZA",
    ];
    if let Some(c) = country
        && (c.len() != 2 || !c.bytes().all(|b| b.is_ascii_uppercase()))
    {
        return Err("Choose a valid country code");
    }
    if let Some(s) = state
        && (country.unwrap_or("NG") != "NG" || !STATES.contains(&s))
    {
        return Err("Choose a valid Nigerian state");
    }
    Ok(())
}

fn is_terminal_status(status: &str) -> bool {
    matches!(
        status,
        "counted" | "refund_pending" | "refunding" | "refunded" | "refund_failed"
    )
}

fn public_status(status: &str) -> &'static str {
    match status {
        "pending" | "verifying" => "payment_pending",
        "counted" => "counted",
        "refund_pending" | "refunding" => "refund_in_progress",
        "refunded" => "refunded",
        "refund_failed" => "refund_needs_attention",
        _ => "not_counted",
    }
}

/// Accepts either the private-path receipt id (`pr_` + 128 random bits) or the
/// Paystack-derived public ballot code (`TP26-XXXXXXXX-XXXXXXXX`). Returns the
/// canonical value to query, or `None` for malformed input.
fn normalize_public_ref(value: &str) -> Option<String> {
    let value = value.trim();
    if let Some(rest) = value.strip_prefix("pr_") {
        return (rest.len() == 32 && rest.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| format!("pr_{}", rest.to_ascii_lowercase()));
    }
    let upper = value.to_ascii_uppercase();
    let parts: Vec<&str> = upper.split('-').collect();
    if parts.len() == 3
        && parts[0] == "TP26"
        && parts[1].len() == 8
        && parts[2].len() == 8
        && parts[1..]
            .iter()
            .all(|part| part.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Some(upper);
    }
    None
}

fn vote_code_for(key: &str, election_id: i64, provider_transaction_id: &str) -> String {
    let digest = hmac256(
        key,
        &format!("vote-code:{}:{}", election_id, provider_transaction_id),
    )
    .to_uppercase();
    format!("TP26-{}-{}", &digest[0..8], &digest[8..16])
}

/// Client address for rate limiting. Behind Caddy the right-most
/// `X-Forwarded-For` entry is the peer Caddy saw; taking the right-most value
/// avoids trusting a header the client prepended.
fn client_key(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok())
        })
        .unwrap_or("unknown")
        .to_owned()
}

fn rate_limited(message: &'static str) -> Response {
    let mut response = ApiError::response(StatusCode::TOO_MANY_REQUESTS, message);
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
    response
}

fn parse_date(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

fn hmac256(key: &str, value: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(key.as_bytes()).expect("HMAC accepts keys of any length");
    mac.update(value.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn verify_hmac512(key: &str, body: &[u8], signature: &str) -> bool {
    let mut mac = match HmacSha512::new_from_slice(key.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body);
    let expected = hex::encode(mac.finalize().into_bytes());
    ct_eq(
        expected.to_lowercase().as_bytes(),
        signature.to_lowercase().as_bytes(),
    )
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && bool::from(a.ct_eq(b))
}

fn nonempty_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|v| !v.trim().is_empty())
}
fn env_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(default)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install signal handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {}, }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::Query, routing::get};
    use sqlx::sqlite::SqlitePoolOptions;
    use std::{collections::HashMap, sync::Arc};
    use tokio::{sync::Mutex, task::JoinHandle};

    #[derive(Clone, Default)]
    struct MockPaystack {
        transactions: Arc<Mutex<HashMap<String, Value>>>,
        refund_requests: Arc<Mutex<Vec<Value>>>,
        provider_requests: Arc<Mutex<Vec<Value>>>,
    }

    async fn mock_verify(
        State(mock): State<MockPaystack>,
        Path(reference): Path<String>,
    ) -> Json<Value> {
        let tx = mock.transactions.lock().await.get(&reference).cloned();
        match tx {
            Some(data) => Json(json!({"status":true,"message":"ok","data":data})),
            None => Json(json!({"status":false,"message":"not found"})),
        }
    }

    async fn mock_initialize(
        State(mock): State<MockPaystack>,
        Json(value): Json<Value>,
    ) -> Json<Value> {
        mock.provider_requests.lock().await.push(value.clone());
        Json(
            json!({"status":true,"data":{"reference":value["reference"],"authorization_url":"https://checkout.paystack.com/mock-session"}}),
        )
    }

    async fn mock_charge(
        State(mock): State<MockPaystack>,
        Json(value): Json<Value>,
    ) -> Json<Value> {
        mock.provider_requests.lock().await.push(value.clone());
        let expiry = value
            .pointer("/bank_transfer/account_expires_at")
            .cloned()
            .unwrap_or(Value::Null);
        Json(
            json!({"status":true,"data":{"reference":value["reference"],"display_text":"Transfer exact amount","bank":{"name":"Test Bank","account_expires_at":expiry},"account_number":"0123456789","account_name":"Paystack temporary account"}}),
        )
    }

    async fn mock_refund(
        State(mock): State<MockPaystack>,
        Json(value): Json<Value>,
    ) -> Json<Value> {
        mock.refund_requests.lock().await.push(value.clone());
        Json(json!({"status":true,"data":{"status":"pending","reference":value["reference"]}}))
    }

    async fn mock_refund_list(
        State(_mock): State<MockPaystack>,
        Query(_query): Query<HashMap<String, String>>,
    ) -> Json<Value> {
        Json(json!({"status":true,"data":[]}))
    }

    async fn mock_server() -> (String, MockPaystack, JoinHandle<()>) {
        let mock = MockPaystack::default();
        let app = Router::new()
            .route("/transaction/verify/{reference}", get(mock_verify))
            .route("/transaction/initialize", post(mock_initialize))
            .route("/charge", post(mock_charge))
            .route("/refund", post(mock_refund).get(mock_refund_list))
            .with_state(mock.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{}", address), mock, server)
    }

    async fn test_state(api_base: String) -> AppState {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        seed_elections(&db).await.unwrap();
        let now = Utc::now();
        sqlx::query("UPDATE elections SET included=1,roster_status='ready',open_at=?,close_at=? WHERE slug='president-2027'")
            .bind((now - ChronoDuration::minutes(5)).to_rfc3339())
            .bind((now + ChronoDuration::hours(1)).to_rfc3339())
            .execute(&db).await.unwrap();
        AppState {
            db,
            http: Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
            limits: RateLimits::default(),
            config: Config {
                bind: "127.0.0.1:0".into(),
                database_url: "sqlite::memory:".into(),
                public_base_url: "https://polls.example.test".into(),
                api_public_url: "https://api.polls.example.test".into(),
                paystack_api_base: api_base,
                paystack_secret: Some("sk_live_test_fixture".into()),
                synthetic_domain: "receipts.invalid".into(),
                device_hmac_key: Some("device-test-key".into()),
                card_fingerprint_hmac_key: Some("card-test-key".into()),
                admin_password_hash: Some("test-hash".into()),
                admin_session_hmac_key: Some("admin-session-test-key".into()),
                admin_username: "admin".into(),
                cookie_secure: true,
                paid_polling_enabled: true,
                demo_mode: false,
                fee_pass_through_confirmed: true,
                legal_review_approved: true,
                paystack_live_approved: true,
                roster_verification_approved: true,
                payment_refund_rehearsals_complete: true,
                production_domain_configured: true,
                backups_and_restore_verified: true,
                refund_reserve_ready: true,
                admin_controls_verified: true,
            },
        }
    }

    async fn insert_payment(
        state: &AppState,
        reference: &str,
        channel: &str,
        requested: i64,
        paid_at: &str,
        expiry: Option<String>,
    ) -> i64 {
        let election_id: i64 =
            sqlx::query_scalar("SELECT id FROM elections WHERE slug='president-2027'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        let candidate_id: i64 = sqlx::query_scalar(
            "SELECT id FROM candidates WHERE election_id=? ORDER BY party LIMIT 1",
        )
        .bind(election_id)
        .fetch_one(&state.db)
        .await
        .unwrap();
        let receipt = format!("pr_{}", hex::encode(random::<[u8; 16]>()));
        let now = Utc::now().to_rfc3339();
        let b = sqlx::query("INSERT INTO ballots(receipt_id,election_id,candidate_id,location,status,accepted_terms_version,accepted_terms_at,device_fingerprint,created_at) VALUES(?,?,?,'NG:LA','pending',?,?,?,?)")
            .bind(receipt).bind(election_id).bind(candidate_id).bind(TERMS_VERSION).bind(&now).bind(hmac256("device-test-key","device"))
            .bind(&now).execute(&state.db).await.unwrap().last_insert_rowid();
        let transfer_expiry = if channel == "transfer" {
            expiry.or(Some(
                (Utc::now() + ChronoDuration::minutes(30)).to_rfc3339(),
            ))
        } else {
            None
        };
        sqlx::query("INSERT INTO payments(ballot_id,reference,base_amount_kobo,requested_amount_kobo,channel,transfer_expires_at,support_purge_at,created_at,updated_at,paid_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
            .bind(b).bind(reference).bind(BASE_AMOUNT_KOBO).bind(requested).bind(channel).bind(transfer_expiry)
            .bind((Utc::now()+ChronoDuration::days(365)).to_rfc3339()).bind(&now).bind(&now).bind(paid_at)
            .execute(&state.db).await.unwrap();
        b
    }

    #[allow(clippy::too_many_arguments)]
    async fn put_verified(
        mock: &MockPaystack,
        reference: &str,
        id: i64,
        channel: &str,
        amount: i64,
        requested: i64,
        paid_at: &str,
        signature: Option<&str>,
        currency: &str,
    ) {
        let authorization = signature
            .map(|s| json!({"signature":s}))
            .unwrap_or(Value::Null);
        mock.transactions.lock().await.insert(
            reference.to_owned(),
            json!({
                "id":id,"reference":reference,"status":"success","amount":amount,
                "requested_amount":requested,"currency":currency,"paid_at":paid_at,
                "channel":channel,"fees":0,"authorization":authorization
            }),
        );
    }

    async fn response_json(response: Response) -> Value {
        let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn launch_gates_require_live_mode_security_and_each_signoff() {
        let mut state = test_state("http://127.0.0.1:1".into()).await;
        assert!(state.config.launch_gates_complete());

        state.config.paystack_secret = Some("sk_test_fixture".into());
        assert!(!state.config.launch_gates_complete());
        state.config.paystack_secret = Some("sk_live_test_fixture".into());

        state.config.cookie_secure = false;
        assert!(!state.config.launch_gates_complete());
        state.config.cookie_secure = true;

        state.config.legal_review_approved = false;
        assert!(!state.config.launch_gates_complete());
    }

    #[test]
    fn public_receipts_accept_random_id_and_paystack_derived_ballot_code() {
        let id = format!("pr_{}", hex::encode(random::<[u8; 16]>()));
        assert_eq!(normalize_public_ref(&id), Some(id.clone()));
        assert_eq!(normalize_public_ref("pr_short"), None);
        assert_eq!(
            normalize_public_ref(&format!("pr_{}x", "0".repeat(32))),
            None
        );

        let code = vote_code_for("card-test-key", 1, "123456789");
        assert!(normalize_public_ref(&code).is_some());
        assert!(normalize_public_ref(&code.to_lowercase()).is_some());
        assert_eq!(normalize_public_ref("TP26-1234567-12345678"), None);
        assert_eq!(normalize_public_ref("TP27-12345678-12345678"), None);
        assert_ne!(vote_code_for("card-test-key", 2, "123456789"), code);
    }

    #[test]
    fn transfer_fee_gross_up_rounds_up_with_one_kobo_buffer() {
        assert_eq!(transfer_gross_up(10_000), 10_154);
        assert!(transfer_gross_up(10_000) > 10_000);
    }

    #[test]
    fn webhook_signature_uses_raw_body_hmac_sha512() {
        let body = br#"{"event":"charge.success"}"#;
        let mut mac = HmacSha512::new_from_slice(b"secret").unwrap();
        mac.update(body);
        let signature = hex::encode(mac.finalize().into_bytes());
        assert!(verify_hmac512("secret", body, &signature));
        assert!(!verify_hmac512("wrong", body, &signature));
        assert!(!verify_hmac512(
            "secret",
            br#"{"event":"charge.failed"}"#,
            &signature
        ));
    }

    #[test]
    fn consent_location_validation_accepts_optional_ng_state_and_rejects_invalid() {
        assert!(validate_location(None, None).is_ok());
        assert!(validate_location(Some("NG"), Some("LA")).is_ok());
        assert!(validate_location(Some("NG"), Some("XX")).is_err());
        assert!(validate_location(Some("US"), Some("LA")).is_err());
    }

    #[tokio::test]
    async fn concurrent_callback_settlement_counts_one_ballot_once() {
        let (base, mock, server) = mock_server().await;
        let state = test_state(base).await;
        let reference = "poll_concurrent_1";
        let paid_at = Utc::now().to_rfc3339();
        insert_payment(&state, reference, "card", BASE_AMOUNT_KOBO, &paid_at, None).await;
        put_verified(
            &mock,
            reference,
            1001,
            "card",
            BASE_AMOUNT_KOBO,
            BASE_AMOUNT_KOBO,
            &paid_at,
            Some("sig-one"),
            "NGN",
        )
        .await;
        let (left, right) = tokio::join!(
            settle_reference(&state, reference, None),
            settle_reference(&state, reference, None)
        );
        assert!(left.is_ok() && right.is_ok());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ballots WHERE status='counted'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 1);
        assert!(mock.refund_requests.lock().await.is_empty());
        server.abort();
    }

    #[tokio::test]
    async fn card_signature_duplicate_requests_a_full_idempotent_refund() {
        let (base, mock, server) = mock_server().await;
        let state = test_state(base).await;
        let paid_at = Utc::now().to_rfc3339();
        for (reference, id) in [("poll_card_a", 2001), ("poll_card_b", 2002)] {
            insert_payment(&state, reference, "card", BASE_AMOUNT_KOBO, &paid_at, None).await;
            put_verified(
                &mock,
                reference,
                id,
                "card",
                BASE_AMOUNT_KOBO,
                BASE_AMOUNT_KOBO,
                &paid_at,
                Some("same-card-signature"),
                "NGN",
            )
            .await;
            settle_reference(&state, reference, None).await.unwrap();
        }
        let counted: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ballots WHERE status='counted'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        let refunding: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ballots WHERE status='refunding'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(counted, 1);
        assert_eq!(refunding, 1);
        let requests = mock.refund_requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0].get("amount").is_none(),
            "full refund is requested with Paystack's full-refund semantics"
        );
        let stable_reference = requests[0]["reference"].as_str().unwrap();
        assert!(stable_reference.starts_with("rf_"));
        drop(requests);
        server.abort();
    }

    #[tokio::test]
    async fn missing_card_signature_is_soft_and_transfer_expiry_is_hard() {
        let (base, mock, server) = mock_server().await;
        let state = test_state(base).await;
        let paid_at = Utc::now().to_rfc3339();
        for (reference, id) in [("poll_no_sig_a", 3001), ("poll_no_sig_b", 3002)] {
            insert_payment(&state, reference, "card", BASE_AMOUNT_KOBO, &paid_at, None).await;
            put_verified(
                &mock,
                reference,
                id,
                "card",
                BASE_AMOUNT_KOBO,
                BASE_AMOUNT_KOBO,
                &paid_at,
                None,
                "NGN",
            )
            .await;
            settle_reference(&state, reference, None).await.unwrap();
        }
        let counted_cards: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ballots WHERE status='counted'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(counted_cards, 2);

        let expiry = (Utc::now() - ChronoDuration::minutes(1)).to_rfc3339();
        let transfer_paid = Utc::now().to_rfc3339();
        insert_payment(
            &state,
            "poll_transfer_expired",
            "transfer",
            transfer_gross_up(BASE_AMOUNT_KOBO),
            &transfer_paid,
            Some(expiry),
        )
        .await;
        put_verified(
            &mock,
            "poll_transfer_expired",
            3003,
            "bank",
            transfer_gross_up(BASE_AMOUNT_KOBO),
            transfer_gross_up(BASE_AMOUNT_KOBO),
            &transfer_paid,
            None,
            "NGN",
        )
        .await;
        settle_reference(&state, "poll_transfer_expired", None)
            .await
            .unwrap();
        let status: String = sqlx::query_scalar("SELECT status FROM ballots WHERE id=(SELECT ballot_id FROM payments WHERE reference='poll_transfer_expired')")
            .fetch_one(&state.db).await.unwrap();
        assert_eq!(status, "refunding");
        assert_eq!(mock.refund_requests.lock().await.len(), 1);
        server.abort();
    }

    #[tokio::test]
    async fn successful_on_time_transfer_is_counted_after_provider_verification() {
        let (base, mock, server) = mock_server().await;
        let state = test_state(base).await;
        let reference = "poll_transfer_success";
        let paid_at = Utc::now().to_rfc3339();
        let requested = transfer_gross_up(BASE_AMOUNT_KOBO);
        let ballot_id = insert_payment(
            &state,
            reference,
            "transfer",
            requested,
            &paid_at,
            Some((Utc::now() + ChronoDuration::minutes(30)).to_rfc3339()),
        )
        .await;
        put_verified(
            &mock, reference, 3501, "bank", requested, requested, &paid_at, None, "NGN",
        )
        .await;
        let provider_fee = 153;
        mock.transactions.lock().await.get_mut(reference).unwrap()["fees"] = json!(provider_fee);

        settle_reference(&state, reference, None).await.unwrap();

        let (status, actual, fee): (String, i64, i64) = sqlx::query_as(
            "SELECT b.status,p.actual_amount_kobo,p.fee_kobo FROM ballots b JOIN payments p ON p.ballot_id=b.id WHERE b.id=?",
        )
        .bind(ballot_id)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(status, "counted");
        assert_eq!(actual, requested);
        assert_eq!(fee, provider_fee);
        assert!(mock.refund_requests.lock().await.is_empty());
        server.abort();
    }

    #[tokio::test]
    async fn late_mismatched_and_wrong_currency_payments_are_refunded() {
        let (base, mock, server) = mock_server().await;
        let state = test_state(base).await;
        let now = Utc::now();
        let close = (now - ChronoDuration::seconds(10)).to_rfc3339();
        sqlx::query("UPDATE elections SET close_at=? WHERE slug='president-2027'")
            .bind(close)
            .execute(&state.db)
            .await
            .unwrap();
        let late = now.to_rfc3339();
        insert_payment(&state, "poll_late", "card", BASE_AMOUNT_KOBO, &late, None).await;
        put_verified(
            &mock,
            "poll_late",
            4001,
            "card",
            BASE_AMOUNT_KOBO,
            BASE_AMOUNT_KOBO,
            &late,
            None,
            "NGN",
        )
        .await;
        settle_reference(&state, "poll_late", None).await.unwrap();

        sqlx::query("UPDATE elections SET close_at=? WHERE slug='president-2027'")
            .bind((Utc::now() + ChronoDuration::hours(1)).to_rfc3339())
            .execute(&state.db)
            .await
            .unwrap();
        let paid_at = Utc::now().to_rfc3339();
        insert_payment(
            &state,
            "poll_mismatch",
            "card",
            BASE_AMOUNT_KOBO,
            &paid_at,
            None,
        )
        .await;
        put_verified(
            &mock,
            "poll_mismatch",
            4002,
            "card",
            9_000,
            9_000,
            &paid_at,
            None,
            "NGN",
        )
        .await;
        settle_reference(&state, "poll_mismatch", None)
            .await
            .unwrap();

        insert_payment(
            &state,
            "poll_currency",
            "card",
            BASE_AMOUNT_KOBO,
            &paid_at,
            None,
        )
        .await;
        put_verified(
            &mock,
            "poll_currency",
            4003,
            "card",
            BASE_AMOUNT_KOBO,
            BASE_AMOUNT_KOBO,
            &paid_at,
            None,
            "USD",
        )
        .await;
        settle_reference(&state, "poll_currency", None)
            .await
            .unwrap();
        let refunding: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ballots WHERE status='refunding'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(refunding, 3);
        assert_eq!(mock.refund_requests.lock().await.len(), 3);
        server.abort();
    }

    #[tokio::test]
    async fn vote_initialization_keeps_candidate_out_of_provider_and_sets_transfer_total() {
        let (base, mock, server) = mock_server().await;
        let state = test_state(base).await;
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-device-token",
            HeaderValue::from_static("0123456789abcdef0123456789abcdef"),
        );
        let card = VoteInitRequest {
            election: "president-2027".into(),
            candidate_party: "AA".into(),
            country_code: None,
            state_code: None,
            accepted_terms: true,
            terms_version: TERMS_VERSION.into(),
            channel: "card".into(),
        };
        let card_response = init_vote(State(state.clone()), headers.clone(), Json(card)).await;
        assert_eq!(card_response.status(), StatusCode::OK);
        let card_json = response_json(card_response).await;
        assert_eq!(card_json["base_amount_kobo"], BASE_AMOUNT_KOBO);
        assert!(
            card_json["checkout_url"]
                .as_str()
                .unwrap()
                .starts_with("https://checkout.paystack.com/")
        );
        let transfer = VoteInitRequest {
            election: "president-2027".into(),
            candidate_party: "AAC".into(),
            country_code: Some("NG".into()),
            state_code: Some("LA".into()),
            accepted_terms: true,
            terms_version: TERMS_VERSION.into(),
            channel: "transfer".into(),
        };
        let transfer_response = init_vote(State(state.clone()), headers, Json(transfer)).await;
        assert_eq!(transfer_response.status(), StatusCode::OK);
        let transfer_json = response_json(transfer_response).await;
        assert_eq!(
            transfer_json["requested_amount_kobo"],
            transfer_gross_up(BASE_AMOUNT_KOBO)
        );
        assert_eq!(
            transfer_json["estimated_fee_kobo"],
            transfer_gross_up(BASE_AMOUNT_KOBO) - BASE_AMOUNT_KOBO
        );
        let requests = mock.provider_requests.lock().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0]["amount"], "10000");
        assert_eq!(requests[0]["channels"][0], "card");
        assert!(requests[0].get("metadata").is_none());
        assert_eq!(
            requests[1]["amount"],
            transfer_gross_up(BASE_AMOUNT_KOBO).to_string()
        );
        assert!(requests[1].get("metadata").is_none());
        assert!(
            requests[0]["email"]
                .as_str()
                .unwrap()
                .ends_with("@receipts.invalid")
        );
        server.abort();
    }

    #[tokio::test]
    async fn raw_webhook_signature_is_required_and_duplicate_event_is_idempotent() {
        let (base, mock, server) = mock_server().await;
        let state = test_state(base).await;
        let reference = "poll_webhook";
        let paid_at = Utc::now().to_rfc3339();
        insert_payment(&state, reference, "card", BASE_AMOUNT_KOBO, &paid_at, None).await;
        put_verified(
            &mock,
            reference,
            5001,
            "card",
            BASE_AMOUNT_KOBO,
            BASE_AMOUNT_KOBO,
            &paid_at,
            None,
            "NGN",
        )
        .await;
        let raw = Bytes::from(
            serde_json::to_vec(
                &json!({"event":"charge.success","data":{"reference":reference,"id":5001}}),
            )
            .unwrap(),
        );
        let bad = paystack_webhook(
            State(state.clone()),
            HeaderMap::from_iter([(
                "x-paystack-signature".parse().unwrap(),
                HeaderValue::from_static("bad"),
            )]),
            raw.clone(),
        )
        .await;
        assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);
        let mut mac = HmacSha512::new_from_slice(b"sk_live_test_fixture").unwrap();
        mac.update(&raw);
        let signature = hex::encode(mac.finalize().into_bytes());
        let headers = HeaderMap::from_iter([(
            "x-paystack-signature".parse().unwrap(),
            HeaderValue::from_str(&signature).unwrap(),
        )]);
        assert_eq!(
            paystack_webhook(State(state.clone()), headers.clone(), raw.clone())
                .await
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            paystack_webhook(State(state.clone()), headers, raw)
                .await
                .status(),
            StatusCode::OK
        );
        let counted: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ballots WHERE status='counted'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM webhook_events")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(counted, 1);
        assert_eq!(events, 1);
        server.abort();
    }

    #[tokio::test]
    async fn rejected_transfer_is_abandoned_and_receipt_never_contains_candidate_or_payment_ids() {
        let (base, _mock, server) = mock_server().await;
        let state = test_state(base).await;
        let reference = "poll_rejected";
        let paid_at = Utc::now().to_rfc3339();
        let ballot_id = insert_payment(
            &state,
            reference,
            "transfer",
            transfer_gross_up(BASE_AMOUNT_KOBO),
            &paid_at,
            Some((Utc::now() + ChronoDuration::minutes(30)).to_rfc3339()),
        )
        .await;
        let raw = Bytes::from(
            serde_json::to_vec(
                &json!({"event":"bank.transfer.rejected","data":{"reference":reference}}),
            )
            .unwrap(),
        );
        let mut mac = HmacSha512::new_from_slice(b"sk_live_test_fixture").unwrap();
        mac.update(&raw);
        let signature = hex::encode(mac.finalize().into_bytes());
        let headers = HeaderMap::from_iter([(
            "x-paystack-signature".parse().unwrap(),
            HeaderValue::from_str(&signature).unwrap(),
        )]);
        assert_eq!(
            paystack_webhook(State(state.clone()), headers, raw)
                .await
                .status(),
            StatusCode::OK
        );
        let status: String = sqlx::query_scalar("SELECT status FROM ballots WHERE id=?")
            .bind(ballot_id)
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(status, "abandoned");

        let receipt: String = sqlx::query_scalar("SELECT receipt_id FROM ballots WHERE id=?")
            .bind(ballot_id)
            .fetch_one(&state.db)
            .await
            .unwrap();
        let body = response_json(get_receipt(State(state.clone()), Path(receipt)).await).await;
        assert!(body.get("candidate").is_none());
        assert!(body.get("candidate_id").is_none());
        assert!(body.get("reference").is_none());
        assert!(body.get("payment_id").is_none());
        server.abort();
    }

    #[tokio::test]
    async fn admin_sessions_require_cookie_and_csrf_token() {
        let (base, _, server) = mock_server().await;
        let state = test_state(base).await;
        let token = "test-admin-session-token";
        let csrf = "test-csrf-secret";
        let token_hash = hmac256("admin-session-test-key", token);
        let csrf_hash = hmac256("admin-session-test-key", csrf);
        let admin_id = sqlx::query(
            "INSERT INTO admins(username,password_hash,active) VALUES('admin','hash',1)",
        )
        .execute(&state.db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query("INSERT INTO admin_sessions(token_hash,admin_id,csrf_hash,expires_at,created_at) VALUES(?,?,?,?,?)")
            .bind(token_hash).bind(admin_id).bind(csrf_hash).bind((Utc::now()+ChronoDuration::hours(1)).to_rfc3339()).bind(Utc::now().to_rfc3339())
            .execute(&state.db).await.unwrap();
        assert!(
            admin_session(&state, &HeaderMap::new(), false)
                .await
                .is_none()
        );
        let mut no_csrf = HeaderMap::new();
        no_csrf.insert(
            header::COOKIE,
            HeaderValue::from_static("polls_admin=test-admin-session-token"),
        );
        assert!(admin_session(&state, &no_csrf, true).await.is_none());
        no_csrf.insert("x-csrf-token", HeaderValue::from_static("wrong"));
        assert!(admin_session(&state, &no_csrf, true).await.is_none());
        no_csrf.insert("x-csrf-token", HeaderValue::from_static("test-csrf-secret"));
        assert!(admin_session(&state, &no_csrf, true).await.is_some());
        let paid_at = Utc::now().to_rfc3339();
        insert_payment(
            &state,
            "poll_soft_signal_a",
            "card",
            BASE_AMOUNT_KOBO,
            &paid_at,
            None,
        )
        .await;
        insert_payment(
            &state,
            "poll_soft_signal_b",
            "card",
            BASE_AMOUNT_KOBO,
            &paid_at,
            None,
        )
        .await;
        let signal_body =
            response_json(admin_risk_signals(State(state.clone()), no_csrf).await).await;
        assert_eq!(signal_body["signals"][0]["repeated_device_groups"], 1);
        assert_eq!(signal_body["signals"][0]["ballots_with_repeats"], 2);
        assert!(signal_body.get("device_fingerprint").is_none());
        server.abort();
    }

    #[tokio::test]
    async fn underpaid_card_is_refunded_even_when_the_fee_would_close_the_gap() {
        let (base, mock, server) = mock_server().await;
        let state = test_state(base).await;
        let reference = "poll_underpaid";
        let paid_at = Utc::now().to_rfc3339();
        insert_payment(&state, reference, "card", BASE_AMOUNT_KOBO, &paid_at, None).await;
        put_verified(
            &mock,
            reference,
            6001,
            "card",
            9_900,
            BASE_AMOUNT_KOBO,
            &paid_at,
            Some("sig-underpaid"),
            "NGN",
        )
        .await;
        mock.transactions.lock().await.get_mut(reference).unwrap()["fees"] = json!(153);

        settle_reference(&state, reference, None).await.unwrap();

        let (status, actual): (String, i64) = sqlx::query_as(
            "SELECT b.status,p.actual_amount_kobo FROM ballots b JOIN payments p ON p.ballot_id=b.id WHERE p.reference=?",
        )
        .bind(reference)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(status, "refunding");
        assert_eq!(actual, 9_900);
        assert_eq!(mock.refund_requests.lock().await.len(), 1);
        server.abort();
    }

    #[tokio::test]
    async fn settled_ballot_exposes_a_paystack_derived_code_that_finds_the_receipt() {
        let (base, mock, server) = mock_server().await;
        let state = test_state(base).await;
        let reference = "poll_identity";
        let paid_at = Utc::now().to_rfc3339();
        insert_payment(&state, reference, "card", BASE_AMOUNT_KOBO, &paid_at, None).await;
        put_verified(
            &mock,
            reference,
            7001,
            "card",
            BASE_AMOUNT_KOBO,
            BASE_AMOUNT_KOBO,
            &paid_at,
            Some("sig-identity"),
            "NGN",
        )
        .await;
        settle_reference(&state, reference, None).await.unwrap();

        let code: String = sqlx::query_scalar(
            "SELECT b.vote_code FROM ballots b JOIN payments p ON p.ballot_id=b.id WHERE p.reference=?",
        )
        .bind(reference)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(code, vote_code_for("card-test-key", 1, "7001"));
        let body = response_json(get_receipt(State(state.clone()), Path(code.clone())).await).await;
        assert_eq!(body["status"], "counted");
        assert_eq!(body["ballot_code"], code);
        assert!(body.get("candidate").is_none());
        assert!(body.get("reference").is_none());
        server.abort();
    }

    #[tokio::test]
    async fn second_settlement_of_a_counted_ballot_never_requests_a_refund() {
        let (base, mock, server) = mock_server().await;
        let state = test_state(base).await;
        let reference = "poll_resettle";
        let paid_at = Utc::now().to_rfc3339();
        insert_payment(&state, reference, "card", BASE_AMOUNT_KOBO, &paid_at, None).await;
        put_verified(
            &mock,
            reference,
            8001,
            "card",
            BASE_AMOUNT_KOBO,
            BASE_AMOUNT_KOBO,
            &paid_at,
            Some("sig-resettle"),
            "NGN",
        )
        .await;
        for _ in 0..3 {
            settle_reference(&state, reference, None).await.unwrap();
        }
        let counted: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ballots WHERE status='counted'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(counted, 1);
        assert!(mock.refund_requests.lock().await.is_empty());
        server.abort();
    }

    #[test]
    fn demo_mode_accepts_only_a_test_key_and_still_requires_enablement() {
        let mut config = Config {
            bind: "127.0.0.1:0".into(),
            database_url: "sqlite::memory:".into(),
            public_base_url: "http://127.0.0.1:3000".into(),
            api_public_url: "http://localhost:8080".into(),
            paystack_api_base: "https://api.paystack.co".into(),
            paystack_secret: Some("sk_test_fixture".into()),
            synthetic_domain: "receipts.invalid".into(),
            device_hmac_key: Some("device".into()),
            card_fingerprint_hmac_key: Some("card".into()),
            admin_password_hash: None,
            admin_session_hmac_key: None,
            admin_username: "admin".into(),
            cookie_secure: false,
            paid_polling_enabled: true,
            demo_mode: true,
            fee_pass_through_confirmed: true,
            legal_review_approved: false,
            paystack_live_approved: false,
            roster_verification_approved: false,
            payment_refund_rehearsals_complete: false,
            production_domain_configured: false,
            backups_and_restore_verified: false,
            refund_reserve_ready: false,
            admin_controls_verified: false,
        };
        assert!(config.payments_ready());
        assert!(!config.launch_gates_complete());
        config.paystack_secret = Some("sk_live_fixture".into());
        assert!(!config.payments_ready());
        config.paystack_secret = Some("sk_test_fixture".into());
        config.paid_polling_enabled = false;
        assert!(!config.payments_ready());
    }

    #[test]
    fn rate_limiter_blocks_after_the_window_limit() {
        let limits = RateLimits::default();
        assert!(limits.allow("ip:1", 2, Duration::from_secs(60)));
        assert!(limits.allow("ip:1", 2, Duration::from_secs(60)));
        assert!(!limits.allow("ip:1", 2, Duration::from_secs(60)));
        assert!(limits.allow("ip:2", 2, Duration::from_secs(60)));
    }

    #[tokio::test]
    async fn retention_detaches_candidate_location_then_purges_payment_support_data() {
        let (base, _, server) = mock_server().await;
        let state = test_state(base).await;
        let closed = (Utc::now() - ChronoDuration::days(366)).to_rfc3339();
        sqlx::query("UPDATE elections SET close_at=?,open_at=? WHERE slug='president-2027'")
            .bind(&closed)
            .bind(&closed)
            .execute(&state.db)
            .await
            .unwrap();
        let reference = "poll_retention";
        let ballot_id =
            insert_payment(&state, reference, "card", BASE_AMOUNT_KOBO, &closed, None).await;
        sqlx::query("UPDATE ballots SET status='counted',counted_at=? WHERE id=?")
            .bind(&closed)
            .bind(ballot_id)
            .execute(&state.db)
            .await
            .unwrap();
        run_retention(&state.db).await.unwrap();
        let (candidate_id, location, consent_at, status): (
            Option<i64>,
            Option<String>,
            Option<String>,
            String,
        ) = sqlx::query_as(
            "SELECT candidate_id,location,accepted_terms_at,status FROM ballots WHERE id=?",
        )
        .bind(ballot_id)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(candidate_id, None);
        assert_eq!(location, None);
        assert_eq!(consent_at, None);
        assert_eq!(status, "counted");
        let payment_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM payments WHERE ballot_id=?")
                .bind(ballot_id)
                .fetch_one(&state.db)
                .await
                .unwrap();
        let archive_count: i64 = sqlx::query_scalar("SELECT SUM(total) FROM tally_archives WHERE election_id=(SELECT id FROM elections WHERE slug='president-2027')").fetch_one(&state.db).await.unwrap();
        assert_eq!(payment_count, 0);
        assert_eq!(archive_count, 1);
        let receipt: String = sqlx::query_scalar("SELECT receipt_id FROM ballots WHERE id=?")
            .bind(ballot_id)
            .fetch_one(&state.db)
            .await
            .unwrap();
        let public = response_json(get_receipt(State(state.clone()), Path(receipt)).await).await;
        assert_eq!(public["status"], "counted");
        assert!(public.get("candidate_id").is_none());
        assert!(public.get("payment_id").is_none());
        server.abort();
    }
}
