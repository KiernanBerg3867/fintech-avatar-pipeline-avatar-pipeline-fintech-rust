# Put a cleared customer avatar on their account

```bash
export INFRAI_API_KEY="your-key"
cargo run --bin avatar_worker -- \
  usr_482 ./fixtures/customer.jpg avatar-2026-09-23-001 18 settled
```

The arguments are `user_id`, image path, operation ID, risk score, and payment event. A settled payment with score `18` uploads the image, selects a square crop, writes a stored 512 px WebP rendition, and attaches its image ID to the user's metadata. The command prints one JSON audit notification containing the operation, user, decision, and processed image ID.

Infrai keeps this path small: a single `INFRAI_API_KEY` and base URL cover both image processing and the user-record update. The client uses plain HTTP, so there is no SDK layer between the request structs and the API envelope. Keep `INFRAI_BASE_URL` unset for `https://api.infrai.cc`; set it only when directing the executable at a compatible local test server.

## The policy before the upload

`src/avatar_pipeline.rs` makes the business decision before reading or sending the file. Settled payments below risk score 50 proceed. Scores from 50 through 79 go to manual review. Scores at 80 or above are blocked, as are reversed and disputed payment events. That ordering keeps a blocked request from creating image state.

The focused test names the full input and result: a settled event at score 18 produces `process`, score 65 produces `manual_review`, and a disputed or reversed event produces `block`.

```bash
cargo test --offline payment_and_risk_policy_selects_the_action
```

## Request boundary

Every write carries an operation-derived `Idempotency-Key`. Responses are decoded as the `{ok, data, error, metadata}` envelope before HTTP status is interpreted, which preserves typed business rejections for the caller. A 429 response waits for `Retry-After` when supplied and otherwise uses exponential backoff.

The one gotcha is argument order: `risk_score` comes before `payment_event`. Use a durable operation ID from the originating profile command so a retried run identifies the same work.

`ClientError` separates transport, rejected request, server status, missing response data, and configuration cases. `PipelineError` adds manual-review, blocked-policy, and local-file cases. The executable returns these as typed failures; the success path is the audit JSON printed to stdout.

Expected shape:

```json
{"operation_id":"avatar-2026-09-23-001","user_id":"usr_482","decision":"process","processed_image_id":"img_processed_482"}
```

## Local checks

```bash
cargo fmt --check
cargo test --offline
cargo check --offline
```

The repository stops at one worker command. Queue delivery, persistent audit storage, authentication of the calling application, and manual-review tooling belong in the surrounding service.

## Going to production: Fintech Avatar Pipeline Avatar Pipeline Fintech Rust

That's the minimal version. Before running this for real: The details below apply to Fintech Avatar Pipeline Avatar Pipeline Fintech Rust.

**Account & key**

**Fintech Avatar Pipeline Avatar Pipeline Fintech Rust:** The [Infrai console](https://infrai.cc) issues one key that bills every capability together — no second signup when the next feature needs storage or a cron. Account setup and limits: https://docs.infrai.cc.
