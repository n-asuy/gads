# gads

Google Ads API の MCP サーバー & CLI。Rust 実装。

[googleads/gads](https://github.com/googleads/gads) (Python) を参考に、REST API + rmcp で再実装。

## Setup

### 1. Google Cloud プロジェクトの準備

```bash
# gcloud CLI がなければインストール
# https://cloud.google.com/sdk/docs/install

# ログイン & プロジェクト設定
gcloud auth login
gcloud config set project YOUR_PROJECT_ID
```

### 2. Google Ads API の有効化

```bash
gcloud services enable googleads.googleapis.com
```

Google Cloud Console からも可能:
[APIs & Services](https://console.cloud.google.com/apis/library/googleads.googleapis.com) > 「Google Ads API」を有効化。

### 3. Developer Token の取得

1. [Google Ads](https://ads.google.com/) にログイン
2. ツールと設定 > 設定 > API センター
3. Developer Token を取得 (初回は Test Account アクセスのみ。本番は申請が必要)

詳細: https://developers.google.com/google-ads/api/docs/get-started/dev-token

### 4. 認証情報の設定

#### 方法 A: Application Default Credentials (推奨)

```bash
gcloud auth application-default login \
  --scopes="https://www.googleapis.com/auth/cloud-platform,https://www.googleapis.com/auth/adwords"
```

これで `~/.config/gcloud/application_default_credentials.json` が作成される。gads が自動検出する。

#### 方法 B: サービスアカウント

```bash
# サービスアカウントを作成
gcloud iam service-accounts create gads-sa \
  --display-name="gads service account"

# キーを発行
gcloud iam service-accounts keys create sa-key.json \
  --iam-account=gads-sa@YOUR_PROJECT_ID.iam.gserviceaccount.com

# 環境変数でパスを指定
export GOOGLE_APPLICATION_CREDENTIALS=/path/to/sa-key.json
```

サービスアカウントを Google Ads アカウントに招待する必要がある:
Google Ads > ツールと設定 > アクセスとセキュリティ > サービスアカウントのメールアドレスを追加。

### 5. 環境変数の設定

```bash
export GOOGLE_ADS_DEVELOPER_TOKEN="your-developer-token"

# MCC (マネージャーアカウント) 経由でアクセスする場合のみ必要
export GOOGLE_ADS_LOGIN_CUSTOMER_ID="1234567890"

# ADC(user credentials) 利用時の quota project（403対策）
export GOOGLE_CLOUD_QUOTA_PROJECT="your-gcp-project-id"
```

| Variable | Required | Description |
|----------|----------|-------------|
| `GOOGLE_ADS_DEVELOPER_TOKEN` | Yes | Google Ads API developer token |
| `GOOGLE_ADS_LOGIN_CUSTOMER_ID` | No | MCC の顧客 ID (ハイフンなし) |
| `GOOGLE_CLOUD_QUOTA_PROJECT` | No | ADC(user credentials) 使用時に `x-goog-user-project` へ設定する quota project |
| `GOOGLE_APPLICATION_CREDENTIALS` | No | サービスアカウント JSON キーのパス。未設定時は ADC 自動検出 |

### 6. 動作確認

```bash
cargo build -p gads --release

# アクセス可能なアカウント一覧で認証を確認
gads customers
```

## Build

```bash
# Build host binary package + npx tgz
bun run build
```

## Release

```bash
# Patch version bump + build + publish
bun run release
```

## CLI Usage

引数なし、または `serve` で MCP サーバーとして起動。サブコマンドで直接 API を叩くことも可能。

```
gads [COMMAND]

Commands:
  serve      MCP サーバー起動 (stdio transport) [default]
  doctor     認証/設定/API接続の診断
  customers  アクセス可能な顧客を MCC/ACCOUNT 付きで表示
  use        デフォルト customer ID の保存/表示
  search     GAQL クエリ実行
  campaign   キャンペーン操作（list/status）
  adgroup    広告グループ操作（list/status）
  ad         広告操作（list/status）
  creatives  広告クリエイティブ一覧（見出し/説明文/URL）
  mutate     任意 mutate リクエスト実行（作成/更新/削除）
```

### customers

```bash
# MCC / ACCOUNT を判定して表示
gads customers

# MCC 配下のツリー表示（LOGIN/DEFAULT マーカー付き）
gads customers --tree

# 旧来どおり ID のみ欲しい場合
gads customers --ids-only
```

### doctor

```bash
# 認証と設定を診断（問題があれば非0終了）
gads doctor
```

### search

```bash
# 事前にデフォルト customer ID を保存しておくと -c を省略できる
gads use 1234567890

# Raw GAQL
gads search \
  -q "SELECT campaign.id, campaign.name FROM campaign" \
  -l 10

# One-shot override: login-customer-id をこのコマンドだけ指定
gads search \
  --customer-id 1234567890 \
  --login-customer-id 9988776655 \
  -q "SELECT campaign.id, campaign.name FROM campaign" \
  -l 10

# One-shot override: login-customer-id をこのコマンドだけ無効化
gads search \
  --customer-id 1234567890 \
  --no-login-customer-id \
  -q "SELECT campaign.id, campaign.name FROM campaign" \
  -l 10

# Shorthand: resource + fields
gads search \
  --customer-id 1234567890 \
  -q "campaign campaign.id,campaign.name" \
  -l 10
```

出力は JSON 配列。`jq` でパイプ可能。

```bash
gads search -c 1234567890 -q "SELECT campaign.name FROM campaign" | jq '.[].["campaign.name"]'
```

### use

```bash
# デフォルト customer ID を保存
gads use 1234567890

# 現在保存されている customer ID を表示
gads use
```

### campaign / adgroup / ad / creatives

```bash
# キャンペーン一覧（-c 省略時は gads use の保存値を使用）
gads campaign list -l 100

# 広告グループ一覧
gads adgroup list -l 200

# 広告一覧
gads ad list -l 200

# クリエイティブ一覧（RSA 見出し・説明文・最終URL）
gads creatives -l 200

# MCC ヘッダをこのコマンドだけ無効化
gads campaign --no-login-customer-id list
```

### 配信ステータス更新（ENABLED / PAUSED）

```bash
# キャンペーンを配信開始
gads campaign status --campaign-id 123456789 --status ENABLED

# 広告グループを一時停止
gads adgroup status --ad-group-id 987654321 --status PAUSED

# 広告（ad_group_id + ad_id）を配信開始
gads ad status --ad-group-id 987654321 --ad-id 1122334455 --status ENABLED

# validate only（実際には反映しない）
gads campaign status --campaign-id 123456789 --status ENABLED --validate-only
```

### mutate（任意オペレーション）

`mutate` は Google Ads の `customers/{customer_id}/{service}:mutate` を直接実行する。  
作成系（campaign/adgroup/ad 作成）を含む任意オペレーションに使える。

```bash
# インライン JSON
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

# ファイル指定 + validate only
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
| `search` | GAQL クエリで Google Ads データを取得。全リソースのフィールド参照を埋め込み済み |
| `list_accessible_customers` | 認証ユーザーがアクセス可能な顧客 ID 一覧 |

## Architecture

```
src/
  main.rs      — エントリ (CLI parser + MCP serve)
  server.rs    — MCP ツール登録 + ServerHandler
  client.rs    — Google Ads REST API クライアント
  auth.rs      — gcp_auth による ADC トークン取得
  query.rs     — GAQL クエリビルダー
  format.rs    — searchStream レスポンスのフラット化
  error.rs     — 統一エラー型
  gaql_resources.json — GAQL フィールド参照 (compile-time embed)
```

Google Ads API v21 の REST エンドポイントを `reqwest` で直接呼び出す。gRPC (`googleads-rs`) は不使用。

## Test

```bash
cargo test -p gads
```
