# gads

A Google Ads API MCP server & CLI written in Rust.

Calls the Google Ads REST API (v21) directly via `reqwest` — no gRPC dependency.
Inspired by [googleads/gads](https://github.com/googleads/gads) (Python).

## Security Notice

gads authenticates through **Google Cloud Application Default Credentials (ADC)**.
Once you run `gcloud auth application-default login`, the resulting credential file
(`~/.config/gcloud/application_default_credentials.json`) is a **machine-wide, long-lived
OAuth2 refresh token**. Any process on your machine — not just gads — can read this file
and obtain access tokens for every scope you authorized, including the Google Ads API
and any other Google Cloud APIs.

**Use gads at your own risk.** You are responsible for securing your local credentials
and understanding the implications of ADC-based authentication.

See [Roadmap](#roadmap) for planned mitigations.

## Setup

### 1. Google Cloud Project

```bash
# Install gcloud CLI if needed
# https://cloud.google.com/sdk/docs/install

gcloud auth login
gcloud config set project YOUR_PROJECT_ID
```

### 2. Enable the Google Ads API

```bash
gcloud services enable googleads.googleapis.com
```

Or enable via [Cloud Console](https://console.cloud.google.com/apis/library/googleads.googleapis.com).

### 3. Obtain a Developer Token

1. Sign in to [Google Ads](https://ads.google.com/)
2. Tools & Settings > Setup > API Center
3. Copy the developer token (initial access is test-account only; production requires approval)

Details: https://developers.google.com/google-ads/api/docs/get-started/dev-token

### 4. Authentication

#### Option A: Application Default Credentials (recommended)

```bash
gcloud auth application-default login \
  --scopes="https://www.googleapis.com/auth/cloud-platform,https://www.googleapis.com/auth/adwords"
```

This creates `~/.config/gcloud/application_default_credentials.json`, which gads discovers automatically.

#### Option B: Service Account

```bash
gcloud iam service-accounts create gads-sa \
  --display-name="gads service account"

gcloud iam service-accounts keys create sa-key.json \
  --iam-account=gads-sa@YOUR_PROJECT_ID.iam.gserviceaccount.com

export GOOGLE_APPLICATION_CREDENTIALS=/path/to/sa-key.json
```

You must also invite the service account email in Google Ads:
Tools & Settings > Access & Security > add the service account email.

### 5. Environment Variables

```bash
export GOOGLE_ADS_DEVELOPER_TOKEN="your-developer-token"

# Required only when accessing accounts via an MCC (Manager account)
export GOOGLE_ADS_LOGIN_CUSTOMER_ID="1234567890"

# Quota project for ADC user credentials (prevents 403)
export GOOGLE_CLOUD_QUOTA_PROJECT="your-gcp-project-id"
```

| Variable | Required | Description |
|----------|----------|-------------|
| `GOOGLE_ADS_DEVELOPER_TOKEN` | Yes | Google Ads API developer token |
| `GOOGLE_ADS_LOGIN_CUSTOMER_ID` | No | MCC customer ID (no hyphens) |
| `GOOGLE_CLOUD_QUOTA_PROJECT` | No | Sets `x-goog-user-project` header for ADC user credentials |
| `GOOGLE_APPLICATION_CREDENTIALS` | No | Path to service account JSON key. Falls back to ADC auto-discovery |

### 6. Verify

```bash
cargo build -p gads --release

# List accessible accounts to confirm authentication
gads customers
```

## Build

```bash
# Build host binary + npx tgz
bun run build
```

## Release

```bash
# Patch version bump + build + publish
bun run release
```

## CLI Usage

Running without arguments (or with `serve`) starts the MCP server.
Subcommands invoke the API directly.

```
gads [COMMAND]

Commands:
  serve      Start MCP server (stdio transport) [default]
  login      Authenticate via gcloud ADC with Google Ads API scope
  doctor     Diagnose auth, config, and API connectivity
  customers  List accessible customers with MCC/ACCOUNT labels
  use        Save/show default customer ID
  search     Execute a GAQL query
  campaign   Campaign operations (list / status)
  adgroup    Ad group operations (list / status)
  ad         Ad operations (list / status)
  creatives  List ad creatives (headlines, descriptions, URLs)
  mutate     Execute arbitrary mutate requests (create/update/delete)
  config     Manage persistent config (developer-token, login-customer-id)
```

### customers

```bash
gads customers
gads customers --tree
gads customers --ids-only
```

### doctor

```bash
gads doctor
```

### search

```bash
# Save a default customer ID first (optional)
gads use 1234567890

# Raw GAQL
gads search \
  -q "SELECT campaign.id, campaign.name FROM campaign" \
  -l 10

# Override login-customer-id for this command only
gads search \
  --customer-id 1234567890 \
  --login-customer-id 9988776655 \
  -q "SELECT campaign.id, campaign.name FROM campaign"

# Disable login-customer-id for this command
gads search \
  --customer-id 1234567890 \
  --no-login-customer-id \
  -q "SELECT campaign.id, campaign.name FROM campaign"

# Shorthand: resource + fields
gads search \
  --customer-id 1234567890 \
  -q "campaign campaign.id,campaign.name" \
  -l 10
```

Output is a JSON array. Pipe to `jq`:

```bash
gads search -c 1234567890 -q "SELECT campaign.name FROM campaign" | jq '.[].["campaign.name"]'
```

### use

```bash
gads use 1234567890   # save default customer ID
gads use              # show current default
```

### campaign / adgroup / ad / creatives

```bash
gads campaign list -l 100
gads adgroup list -l 200
gads ad list -l 200
gads creatives -l 200

# Disable MCC header for this command
gads campaign --no-login-customer-id list
```

### Status updates (ENABLED / PAUSED)

```bash
gads campaign status --campaign-id 123456789 --status ENABLED
gads adgroup status --ad-group-id 987654321 --status PAUSED
gads ad status --ad-group-id 987654321 --ad-id 1122334455 --status ENABLED

# Validate only (dry run)
gads campaign status --campaign-id 123456789 --status ENABLED --validate-only
```

### mutate

Calls `customers/{customer_id}/{service}:mutate` directly.
Supports any create/update/delete operation.

```bash
# Inline JSON
gads mutate \
  --service campaigns \
  --body '{
    "operations": [{
      "update": {
        "resourceName": "customers/1234567890/campaigns/111222333",
        "status": "ENABLED"
      },
      "updateMask": "status"
    }]
  }'

# From file + validate only
gads mutate \
  --service adGroupAds \
  --body-file ./mutate-body.json \
  --validate-only
```

## MCP Server Usage

### Claude Desktop / Claude Code

```json
{
  "mcpServers": {
    "gads": {
      "command": "/path/to/gads",
      "env": {
        "GOOGLE_ADS_DEVELOPER_TOKEN": "YOUR_TOKEN",
        "GOOGLE_ADS_LOGIN_CUSTOMER_ID": "1234567890"
      }
    }
  }
}
```

### MCP Tools

| Tool | Description |
|------|-------------|
| `search` | Execute GAQL queries against Google Ads. Includes embedded field reference for all v21 resources |
| `list_accessible_customers` | List customer IDs accessible by the authenticated user |

## Architecture

```
src/
  main.rs      — Entry point (CLI parser + MCP serve)
  server.rs    — MCP tool registration + ServerHandler
  client.rs    — Google Ads REST API client
  auth.rs      — ADC token acquisition via gcp_auth
  query.rs     — GAQL query builder
  format.rs    — searchStream response flattening
  error.rs     — Unified error type
  gaql_resources.json — GAQL v21 field reference (compile-time embed)
```

## Test

```bash
cargo test -p gads
```

## Roadmap

- **Scoped credential isolation** — Current ADC credentials grant access to all authorized
  scopes machine-wide. Investigate per-tool credential isolation (e.g., short-lived tokens
  with scope restricted to `adwords` only, or a proxy that mediates token exchange) so that
  a compromised process cannot leverage gads credentials for unrelated GCP APIs.
- **Token broker / proxy architecture** — Instead of reading ADC directly, gads could
  request tokens from a local broker that enforces scope, audience, and lifetime constraints.
  This would prevent other processes from reusing the raw refresh token.
- **Credential storage hardening** — Explore alternatives to the plaintext ADC JSON file
  (e.g., OS keychain integration, encrypted-at-rest profiles).
- **Audit logging** — Log all API calls with timestamps and caller context to detect
  unauthorized usage of shared credentials.

## License

MIT
