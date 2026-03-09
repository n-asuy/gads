mod auth;
mod client;
mod error;
mod format;
mod ids;
mod profile;
mod query;
mod server;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use client::GoogleAdsClient;
use ids::parse_customer_id_arg;
use query::GaqlQuery;
use rmcp::{transport::stdio, ServiceExt};
use serde_json::{json, Value};

#[derive(Parser)]
#[command(name = "gads", about = "Google Ads MCP server & CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Args, Clone)]
struct TargetArgs {
    /// Customer ID (numbers only, no hyphens). If omitted, uses `gads use` saved value.
    #[arg(short, long, value_parser = parse_customer_id_arg)]
    customer_id: Option<String>,
    /// Override login-customer-id header for this command only
    #[arg(long, value_parser = parse_customer_id_arg, conflicts_with = "no_login_customer_id")]
    login_customer_id: Option<String>,
    /// Disable login-customer-id header for this command (ignores env var)
    #[arg(long, conflicts_with = "login_customer_id")]
    no_login_customer_id: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Start as MCP server (stdio transport)
    Serve,
    /// List accessible customers with manager/account role hints
    Customers {
        /// Print IDs only (backward-compatible output)
        #[arg(long, conflicts_with = "tree")]
        ids_only: bool,
        /// Print MCC -> child account tree
        #[arg(long, conflicts_with = "ids_only")]
        tree: bool,
    },
    /// Authenticate via gcloud ADC with Google Ads API scope
    Login,
    /// Diagnose local auth/config/API reachability
    Doctor,
    /// Save/show default customer ID used by commands when `--customer-id` is omitted
    Use {
        /// Customer ID to save. If omitted, shows current saved value.
        #[arg(value_parser = parse_customer_id_arg)]
        customer_id: Option<String>,
    },
    /// Execute a GAQL search query
    Search {
        #[command(flatten)]
        target: TargetArgs,
        /// GAQL query string. Example: "SELECT campaign.id, campaign.name FROM campaign"
        #[arg(short, long)]
        query: String,
        /// Maximum number of rows
        #[arg(short, long)]
        limit: Option<u32>,
    },
    /// Campaign operations
    Campaign {
        #[command(flatten)]
        target: TargetArgs,
        #[command(subcommand)]
        command: CampaignCommand,
    },
    /// Ad group operations
    #[command(name = "adgroup")]
    AdGroup {
        #[command(flatten)]
        target: TargetArgs,
        #[command(subcommand)]
        command: AdGroupCommand,
    },
    /// Ad operations
    Ad {
        #[command(flatten)]
        target: TargetArgs,
        #[command(subcommand)]
        command: AdCommand,
    },
    /// List ad creatives (headlines/descriptions/final URLs)
    Creatives {
        #[command(flatten)]
        target: TargetArgs,
        /// Maximum number of rows
        #[arg(short, long, default_value_t = 200)]
        limit: u32,
    },
    /// Execute arbitrary Google Ads mutate request
    Mutate {
        #[command(flatten)]
        target: TargetArgs,
        /// Mutate service name (e.g. campaigns, adGroups, adGroupAds)
        #[arg(long, value_parser = parse_mutate_service_arg)]
        service: String,
        /// JSON body for mutate request
        #[arg(
            long,
            required_unless_present = "body_file",
            conflicts_with = "body_file"
        )]
        body: Option<String>,
        /// Path to JSON body file
        #[arg(long, required_unless_present = "body", conflicts_with = "body")]
        body_file: Option<PathBuf>,
        /// Set validateOnly=true on request body
        #[arg(long)]
        validate_only: bool,
        /// Set partialFailure=true on request body
        #[arg(long)]
        partial_failure: bool,
    },
}

#[derive(Subcommand)]
enum CampaignCommand {
    /// List campaigns
    List {
        /// Maximum number of rows
        #[arg(short, long, default_value_t = 100)]
        limit: u32,
    },
    /// Update campaign status (ENABLED/PAUSED)
    Status {
        /// Campaign ID (numbers only, no hyphens)
        #[arg(long, value_parser = parse_resource_id_arg)]
        campaign_id: String,
        /// Target status
        #[arg(long, value_enum, ignore_case = true)]
        status: DeliveryStatus,
        /// Validate only (do not apply)
        #[arg(long)]
        validate_only: bool,
    },
}

#[derive(Subcommand)]
enum AdGroupCommand {
    /// List ad groups
    List {
        /// Maximum number of rows
        #[arg(short, long, default_value_t = 200)]
        limit: u32,
    },
    /// Update ad group status (ENABLED/PAUSED)
    Status {
        /// Ad Group ID (numbers only, no hyphens)
        #[arg(long, value_parser = parse_resource_id_arg)]
        ad_group_id: String,
        /// Target status
        #[arg(long, value_enum, ignore_case = true)]
        status: DeliveryStatus,
        /// Validate only (do not apply)
        #[arg(long)]
        validate_only: bool,
    },
}

#[derive(Subcommand)]
enum AdCommand {
    /// List ads
    List {
        /// Maximum number of rows
        #[arg(short, long, default_value_t = 200)]
        limit: u32,
    },
    /// Update ad status (ENABLED/PAUSED)
    Status {
        /// Ad Group ID (numbers only, no hyphens)
        #[arg(long, value_parser = parse_resource_id_arg)]
        ad_group_id: String,
        /// Ad ID (numbers only, no hyphens)
        #[arg(long, value_parser = parse_resource_id_arg)]
        ad_id: String,
        /// Target status
        #[arg(long, value_enum, ignore_case = true)]
        status: DeliveryStatus,
        /// Validate only (do not apply)
        #[arg(long)]
        validate_only: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum DeliveryStatus {
    Enabled,
    Paused,
}

impl DeliveryStatus {
    fn as_api_status(self) -> &'static str {
        match self {
            Self::Enabled => "ENABLED",
            Self::Paused => "PAUSED",
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        None | Some(Command::Serve) => run_mcp().await,
        Some(Command::Login) => run_login().await,
        Some(Command::Customers { ids_only, tree }) => run_customers(ids_only, tree).await,
        Some(Command::Doctor) => run_doctor().await,
        Some(Command::Use { customer_id }) => run_use(customer_id),
        Some(Command::Search {
            target,
            query,
            limit,
        }) => run_search(&target, &query, limit).await,
        Some(Command::Campaign { target, command }) => match command {
            CampaignCommand::List { limit } => run_campaigns(&target, limit).await,
            CampaignCommand::Status {
                campaign_id,
                status,
                validate_only,
            } => run_campaign_status(&target, &campaign_id, status, validate_only).await,
        },
        Some(Command::AdGroup { target, command }) => match command {
            AdGroupCommand::List { limit } => run_ad_groups(&target, limit).await,
            AdGroupCommand::Status {
                ad_group_id,
                status,
                validate_only,
            } => run_ad_group_status(&target, &ad_group_id, status, validate_only).await,
        },
        Some(Command::Ad { target, command }) => match command {
            AdCommand::List { limit } => run_ads(&target, limit).await,
            AdCommand::Status {
                ad_group_id,
                ad_id,
                status,
                validate_only,
            } => run_ad_status(&target, &ad_group_id, &ad_id, status, validate_only).await,
        },
        Some(Command::Creatives { target, limit }) => run_creatives(&target, limit).await,
        Some(Command::Mutate {
            target,
            service,
            body,
            body_file,
            validate_only,
            partial_failure,
        }) => {
            run_mutate(
                &target,
                &service,
                body,
                body_file,
                validate_only,
                partial_failure,
            )
            .await
        }
    }
}

async fn run_mcp() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive("gads=info".parse()?),
        )
        .with_writer(std::io::stderr)
        .init();

    let server = server::GadsServer::new().await.map_err(|e| {
        tracing::error!("failed to initialize: {e}");
        e
    })?;

    let service = server.serve(stdio()).await.inspect_err(|e| {
        tracing::error!("failed to start MCP service: {e}");
    })?;

    service.waiting().await?;
    Ok(())
}

async fn run_login() -> Result<(), Box<dyn std::error::Error>> {
    let scopes = &[
        "openid",
        "https://www.googleapis.com/auth/userinfo.email",
        "https://www.googleapis.com/auth/adwords",
    ];

    let mut cmd = tokio::process::Command::new("gcloud");
    cmd.arg("auth")
        .arg("application-default")
        .arg("login")
        .arg("--scopes")
        .arg(scopes.join(","));

    let status = cmd.status().await.map_err(|e| {
        format!("failed to run gcloud: {e}\nMake sure gcloud CLI is installed and in PATH.")
    })?;

    if !status.success() {
        return Err(format!("gcloud exited with {status}").into());
    }

    println!();
    println!("ADC credentials saved with Google Ads scope.");
    println!("Run `gads doctor` to verify connectivity.");
    Ok(())
}

struct CustomerSummary {
    id: String,
    name: Option<String>,
    is_manager: Option<bool>,
}

struct CustomerRelation {
    child: CustomerSummary,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum DoctorStatus {
    Ok,
    Warn,
    Fail,
}

impl DoctorStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
        }
    }
}

async fn run_customers(ids_only: bool, tree: bool) -> Result<(), Box<dyn std::error::Error>> {
    let client = GoogleAdsClient::new().await?;
    let ids = client.list_accessible_customers().await?;

    if ids.is_empty() {
        println!("No accessible customers found.");
        return Ok(());
    }

    if ids_only {
        for id in &ids {
            println!("{id}");
        }
        return Ok(());
    }

    let inspect_client = GoogleAdsClient::new_with_login_customer_id_override(Some(None)).await?;
    let summaries = load_customer_summaries(&inspect_client, &ids).await;

    if tree {
        print_customer_tree(&inspect_client, &summaries).await;
        print_suggested_next_step(&summaries);
        return Ok(());
    }

    println!("Accessible customers:");
    println!("ID\tROLE\tNAME");
    for summary in &summaries {
        let role = match summary.is_manager {
            Some(true) => "MCC",
            Some(false) => "ACCOUNT",
            None => "UNKNOWN",
        };
        let name = summary.name.as_deref().unwrap_or("-");
        println!("{}\t{}\t{}", summary.id, role, name);
    }

    print_suggested_next_step(&summaries);
    Ok(())
}

async fn load_customer_summaries(
    inspect_client: &GoogleAdsClient,
    ids: &[String],
) -> Vec<CustomerSummary> {
    let mut summaries: Vec<CustomerSummary> = Vec::with_capacity(ids.len());
    for id in ids {
        summaries.push(inspect_customer(inspect_client, id).await);
    }
    summaries.sort_by(|a, b| a.id.cmp(&b.id));
    summaries
}

fn print_suggested_next_step(summaries: &[CustomerSummary]) {
    let mcc_ids: Vec<&str> = summaries
        .iter()
        .filter(|s| s.is_manager == Some(true))
        .map(|s| s.id.as_str())
        .collect();
    let account_ids: Vec<&str> = summaries
        .iter()
        .filter(|s| s.is_manager == Some(false))
        .map(|s| s.id.as_str())
        .collect();

    if mcc_ids.is_empty() && account_ids.is_empty() {
        println!();
        println!("Could not classify MCC vs account for any customer.");
        return;
    }

    println!();
    println!("Suggested next step:");
    match (mcc_ids.len(), account_ids.len()) {
        (1, 1) => {
            println!("  export GOOGLE_ADS_LOGIN_CUSTOMER_ID={}", mcc_ids[0]);
            println!("  gads use {}", account_ids[0]);
        }
        (1, n) if n > 1 => {
            println!("  export GOOGLE_ADS_LOGIN_CUSTOMER_ID={}", mcc_ids[0]);
            println!("  Then pick one account ID and run: gads use <ACCOUNT_ID>");
        }
        (m, _) if m > 1 => {
            println!("  Multiple MCCs found. Pick one and set:");
            println!("  export GOOGLE_ADS_LOGIN_CUSTOMER_ID=<MCC_ID>");
            if !account_ids.is_empty() {
                println!("  Then: gads use <ACCOUNT_ID>");
            }
        }
        _ => {
            println!("  No MCC detected. Use direct account mode:");
            println!("  unset GOOGLE_ADS_LOGIN_CUSTOMER_ID");
            if account_ids.len() == 1 {
                println!("  gads use {}", account_ids[0]);
            } else {
                println!("  Then pick one account ID and run: gads use <ACCOUNT_ID>");
            }
        }
    }
}

async fn print_customer_tree(inspect_client: &GoogleAdsClient, summaries: &[CustomerSummary]) {
    let login_marker = normalized_env_customer_id("GOOGLE_ADS_LOGIN_CUSTOMER_ID");
    let default_marker = profile::load_customer_id().ok().flatten();

    println!("Accessible customers (tree):");

    let mut relation_map: BTreeMap<String, Vec<CustomerSummary>> = BTreeMap::new();
    let mut child_seen: BTreeSet<String> = BTreeSet::new();

    for mcc in summaries.iter().filter(|s| s.is_manager == Some(true)) {
        let relations = fetch_customer_relations(inspect_client, &mcc.id).await;
        let mut children = Vec::new();
        for rel in relations {
            child_seen.insert(rel.child.id.clone());
            children.push(rel.child);
        }
        children.sort_by(|a, b| a.id.cmp(&b.id));
        relation_map.insert(mcc.id.clone(), children);
    }

    for mcc in summaries.iter().filter(|s| s.is_manager == Some(true)) {
        println!(
            "MCC {}{}{}",
            mcc.id,
            render_name(mcc.name.as_deref()),
            marker_suffix(&mcc.id, login_marker.as_deref(), default_marker.as_deref())
        );
        match relation_map.get(&mcc.id) {
            Some(children) if !children.is_empty() => {
                for child in children {
                    let role = match child.is_manager {
                        Some(true) => "MCC",
                        Some(false) => "ACCOUNT",
                        None => "UNKNOWN",
                    };
                    println!(
                        "  - {} {}{}{}",
                        role,
                        child.id,
                        render_name(child.name.as_deref()),
                        marker_suffix(
                            &child.id,
                            login_marker.as_deref(),
                            default_marker.as_deref()
                        )
                    );
                }
            }
            _ => println!("  - (no visible child accounts)"),
        }
    }

    let mut leftover: Vec<&CustomerSummary> = summaries
        .iter()
        .filter(|s| !child_seen.contains(&s.id) && s.is_manager != Some(true))
        .collect();
    leftover.sort_by(|a, b| a.id.cmp(&b.id));

    if !leftover.is_empty() {
        println!();
        println!("Directly accessible accounts:");
        for s in leftover {
            let role = match s.is_manager {
                Some(true) => "MCC",
                Some(false) => "ACCOUNT",
                None => "UNKNOWN",
            };
            println!(
                "- {} {}{}{}",
                role,
                s.id,
                render_name(s.name.as_deref()),
                marker_suffix(&s.id, login_marker.as_deref(), default_marker.as_deref())
            );
        }
    }
}

fn render_name(name: Option<&str>) -> String {
    match name {
        Some(n) if !n.trim().is_empty() => format!(" ({n})"),
        _ => String::new(),
    }
}

fn marker_suffix(
    id: &str,
    login_customer_id: Option<&str>,
    default_customer_id: Option<&str>,
) -> String {
    let mut markers = Vec::new();
    if login_customer_id == Some(id) {
        markers.push("LOGIN");
    }
    if default_customer_id == Some(id) {
        markers.push("DEFAULT");
    }
    if markers.is_empty() {
        String::new()
    } else {
        format!(" [{}]", markers.join(","))
    }
}

fn normalized_env_customer_id(key: &str) -> Option<String> {
    let raw = std::env::var(key).ok()?;
    ids::normalize_customer_id(&raw, "env").ok()
}

async fn fetch_customer_relations(client: &GoogleAdsClient, mcc_id: &str) -> Vec<CustomerRelation> {
    const QUERY: &str = "SELECT customer_client.level, customer_client.id, customer_client.client_customer, customer_client.descriptive_name, customer_client.manager \
                         FROM customer_client \
                         WHERE customer_client.level <= 1";
    let response = match client.search_stream(mcc_id, QUERY).await {
        Ok(response) => response,
        Err(_) => return Vec::new(),
    };
    let rows = match format::flatten_search_response(&response) {
        Ok(rows) => rows,
        Err(_) => return Vec::new(),
    };

    let mut out = Vec::new();
    for row in rows {
        let level = row
            .get("customerClient.level")
            .and_then(Value::as_u64)
            .map(|v| v as i64)
            .or_else(|| row.get("customerClient.level").and_then(Value::as_i64))
            .unwrap_or_default();
        if level != 1 {
            continue;
        }

        let client_id = value_as_string(row.get("customerClient.id")).or_else(|| {
            value_as_string(row.get("customerClient.clientCustomer"))
                .and_then(|rn| rn.strip_prefix("customers/").map(|id| id.to_owned()))
        });
        let Some(client_id) = client_id else {
            continue;
        };

        let child = CustomerSummary {
            id: client_id,
            name: value_as_string(row.get("customerClient.descriptiveName"))
                .or_else(|| value_as_string(row.get("customerClient.descriptive_name")))
                .filter(|s| !s.trim().is_empty()),
            is_manager: row.get("customerClient.manager").and_then(Value::as_bool),
        };
        out.push(CustomerRelation { child });
    }
    out
}

async fn run_doctor() -> Result<(), Box<dyn std::error::Error>> {
    println!("gads doctor");
    println!("-----------");

    let mut has_fail = false;
    let mut needs_quota_hint = false;

    match std::env::var("GOOGLE_ADS_DEVELOPER_TOKEN") {
        Ok(v) if !v.trim().is_empty() => {
            doctor_line(
                DoctorStatus::Ok,
                "developer token",
                &format!("set ({})", mask_secret(v.trim())),
            );
        }
        _ => {
            has_fail = true;
            doctor_line(
                DoctorStatus::Fail,
                "developer token",
                "missing GOOGLE_ADS_DEVELOPER_TOKEN",
            );
        }
    }

    match std::env::var("GOOGLE_ADS_LOGIN_CUSTOMER_ID") {
        Ok(v) if v.trim().is_empty() => doctor_line(
            DoctorStatus::Warn,
            "login customer id",
            "set but empty (ignored)",
        ),
        Ok(v) => match ids::normalize_customer_id(&v, "GOOGLE_ADS_LOGIN_CUSTOMER_ID") {
            Ok(id) => doctor_line(DoctorStatus::Ok, "login customer id", &format!("{id}")),
            Err(err) => {
                has_fail = true;
                doctor_line(DoctorStatus::Fail, "login customer id", &err.to_string());
            }
        },
        Err(_) => doctor_line(
            DoctorStatus::Warn,
            "login customer id",
            "unset (direct-account mode only)",
        ),
    }

    match profile::load_customer_id() {
        Ok(Some(id)) => doctor_line(
            DoctorStatus::Ok,
            "default customer",
            &format!("{id} (from gads use)"),
        ),
        Ok(None) => doctor_line(
            DoctorStatus::Warn,
            "default customer",
            "unset (`gads use <CUSTOMER_ID>` to save)",
        ),
        Err(err) => doctor_line(
            DoctorStatus::Warn,
            "default customer",
            &format!("failed to read profile: {err}"),
        ),
    }

    match detect_quota_project_source() {
        Some((project_id, source)) => doctor_line(
            DoctorStatus::Ok,
            "quota project",
            &format!("{project_id} ({source})"),
        ),
        None => {
            needs_quota_hint = true;
            doctor_line(
                DoctorStatus::Warn,
                "quota project",
                "not found (ADC user creds may fail without this)",
            );
        }
    }

    let inspect_client =
        match GoogleAdsClient::new_with_login_customer_id_override(Some(None)).await {
            Ok(client) => {
                doctor_line(
                    DoctorStatus::Ok,
                    "auth/bootstrap",
                    "client init (no-login mode)",
                );
                Some(client)
            }
            Err(err) => {
                has_fail = true;
                doctor_line(DoctorStatus::Fail, "auth/bootstrap", &err.to_string());
                None
            }
        };

    let mut accessible_ids: Vec<String> = Vec::new();
    if let Some(client) = inspect_client.as_ref() {
        match client.list_accessible_customers().await {
            Ok(ids) if ids.is_empty() => {
                doctor_line(DoctorStatus::Warn, "accessible customers", "0 found")
            }
            Ok(ids) => {
                doctor_line(
                    DoctorStatus::Ok,
                    "accessible customers",
                    &format!("{} found", ids.len()),
                );
                accessible_ids = ids;
            }
            Err(err) => {
                has_fail = true;
                doctor_line(DoctorStatus::Fail, "accessible customers", &err.to_string());
            }
        }
    }

    if let (Some(client), Some(first_customer)) = (inspect_client.as_ref(), accessible_ids.first())
    {
        match client
            .search_stream(first_customer, "SELECT customer.id FROM customer LIMIT 1")
            .await
        {
            Ok(_) => doctor_line(
                DoctorStatus::Ok,
                "query (no-login)",
                &format!("customer {first_customer}"),
            ),
            Err(err) => {
                has_fail = true;
                doctor_line(DoctorStatus::Fail, "query (no-login)", &err.to_string());
            }
        }
    }

    if std::env::var("GOOGLE_ADS_LOGIN_CUSTOMER_ID")
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        match GoogleAdsClient::new().await {
            Ok(client) => {
                if let Some(first_customer) = accessible_ids.first() {
                    match client
                        .search_stream(first_customer, "SELECT customer.id FROM customer LIMIT 1")
                        .await
                    {
                        Ok(_) => doctor_line(
                            DoctorStatus::Ok,
                            "query (with login)",
                            &format!("customer {first_customer}"),
                        ),
                        Err(err) => {
                            has_fail = true;
                            doctor_line(DoctorStatus::Fail, "query (with login)", &err.to_string());
                        }
                    }
                } else {
                    doctor_line(
                        DoctorStatus::Warn,
                        "query (with login)",
                        "skipped (no accessible customers)",
                    );
                }
            }
            Err(err) => {
                has_fail = true;
                doctor_line(DoctorStatus::Fail, "query (with login)", &err.to_string());
            }
        }
    }

    if needs_quota_hint {
        println!();
        println!("Hint:");
        println!("  gcloud auth application-default set-quota-project <PROJECT_ID>");
        println!("  export GOOGLE_CLOUD_QUOTA_PROJECT=<PROJECT_ID>");
    }

    println!();
    if has_fail {
        println!("Result: FAIL");
        return Err("doctor found blocking issues".into());
    }
    println!("Result: OK");
    Ok(())
}

fn doctor_line(status: DoctorStatus, name: &str, detail: &str) {
    println!("[{}] {:<20} {detail}", status.as_str(), name);
}

fn mask_secret(value: &str) -> String {
    if value.len() <= 6 {
        return "***".to_owned();
    }
    let suffix = &value[value.len() - 4..];
    format!("***{suffix}")
}

fn detect_quota_project_source() -> Option<(String, String)> {
    for key in ["GOOGLE_CLOUD_QUOTA_PROJECT", "GOOGLE_CLOUD_PROJECT"] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some((trimmed.to_owned(), format!("env:{key}")));
            }
        }
    }

    if let Ok(adc_path) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
        let trimmed = adc_path.trim();
        if !trimmed.is_empty() {
            let path = PathBuf::from(trimmed);
            if let Some(project_id) = read_quota_project_id(&path) {
                return Some((project_id, format!("adc:{}", path.display())));
            }
        }
    }

    let home = std::env::var("HOME").ok()?;
    let default_adc_path =
        PathBuf::from(home).join(".config/gcloud/application_default_credentials.json");
    read_quota_project_id(&default_adc_path)
        .map(|project_id| (project_id, format!("adc:{}", default_adc_path.display())))
}

fn read_quota_project_id(path: &PathBuf) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let json: Value = serde_json::from_str(&content).ok()?;
    let quota_project_id = json.get("quota_project_id")?.as_str()?.trim();
    if quota_project_id.is_empty() {
        None
    } else {
        Some(quota_project_id.to_owned())
    }
}

async fn inspect_customer(client: &GoogleAdsClient, customer_id: &str) -> CustomerSummary {
    const QUERY: &str =
        "SELECT customer.id, customer.descriptive_name, customer.manager FROM customer LIMIT 1";

    let response = match client.search_stream(customer_id, QUERY).await {
        Ok(response) => response,
        Err(_) => {
            return CustomerSummary {
                id: customer_id.to_owned(),
                name: None,
                is_manager: None,
            };
        }
    };

    let rows = match format::flatten_search_response(&response) {
        Ok(rows) => rows,
        Err(_) => {
            return CustomerSummary {
                id: customer_id.to_owned(),
                name: None,
                is_manager: None,
            };
        }
    };

    let row = rows.first();
    let id = row
        .and_then(|r| value_as_string(r.get("customer.id")))
        .unwrap_or_else(|| customer_id.to_owned());
    let name = row
        .and_then(|r| {
            value_as_string(r.get("customer.descriptiveName"))
                .or_else(|| value_as_string(r.get("customer.descriptive_name")))
        })
        .filter(|s| !s.trim().is_empty());
    let is_manager = row
        .and_then(|r| r.get("customer.manager"))
        .and_then(Value::as_bool);

    CustomerSummary {
        id,
        name,
        is_manager,
    }
}

fn value_as_string(v: Option<&Value>) -> Option<String> {
    v.and_then(|x| {
        x.as_str()
            .map(|s| s.to_owned())
            .or_else(|| x.as_i64().map(|n| n.to_string()))
            .or_else(|| x.as_u64().map(|n| n.to_string()))
    })
}

fn run_use(customer_id: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    match customer_id {
        Some(customer_id) => {
            let path = profile::save_customer_id(&customer_id)?;
            println!(
                "Saved default customer ID: {customer_id} ({})",
                path.display()
            );
        }
        None => match profile::load_customer_id()? {
            Some(customer_id) => println!("{customer_id}"),
            None => println!("No default customer ID. Run `gads use <CUSTOMER_ID>` first."),
        },
    }

    Ok(())
}

async fn run_search(
    target: &TargetArgs,
    raw_query: &str,
    limit: Option<u32>,
) -> Result<(), Box<dyn std::error::Error>> {
    let customer_id = resolve_customer_id(target)?;
    let client = client_for_target(target).await?;

    // If the query looks like raw GAQL (starts with SELECT), use it directly.
    // Otherwise, treat it as a simple resource query.
    let gaql = if raw_query.trim_start().to_uppercase().starts_with("SELECT") {
        let mut q = raw_query.to_owned();
        if let Some(limit) = limit {
            if !q.to_uppercase().contains("LIMIT") {
                let _ = write!(q, " LIMIT {limit}");
            }
        }
        q
    } else {
        // Parse as "resource field1,field2,..." shorthand
        let parts: Vec<&str> = raw_query.splitn(2, ' ').collect();
        let (resource, fields): (&str, Vec<String>) = match parts.as_slice() {
            [resource, fields] => (
                *resource,
                fields.split(',').map(|s| s.trim().to_owned()).collect(),
            ),
            [resource] => (*resource, vec![format!("{resource}.resource_name")]),
            _ => return Err("invalid query format".into()),
        };
        GaqlQuery {
            fields: &fields,
            resource,
            conditions: None,
            orderings: None,
            limit,
        }
        .build()?
    };

    run_gaql_query(&client, &customer_id, &gaql).await
}

async fn run_campaigns(target: &TargetArgs, limit: u32) -> Result<(), Box<dyn std::error::Error>> {
    let customer_id = resolve_customer_id(target)?;
    let client = client_for_target(target).await?;
    let query = format!(
        "SELECT campaign.id, campaign.name, campaign.status, campaign.advertising_channel_type, campaign.start_date, campaign.end_date \
         FROM campaign \
         WHERE campaign.status != 'REMOVED' \
         ORDER BY campaign.id DESC \
         LIMIT {limit}"
    );

    run_gaql_query(&client, &customer_id, &query).await
}

async fn run_ad_groups(target: &TargetArgs, limit: u32) -> Result<(), Box<dyn std::error::Error>> {
    let customer_id = resolve_customer_id(target)?;
    let client = client_for_target(target).await?;
    let query = format!(
        "SELECT campaign.id, campaign.name, ad_group.id, ad_group.name, ad_group.status, ad_group.type \
         FROM ad_group \
         WHERE ad_group.status != 'REMOVED' \
         ORDER BY ad_group.id DESC \
         LIMIT {limit}"
    );

    run_gaql_query(&client, &customer_id, &query).await
}

async fn run_ads(target: &TargetArgs, limit: u32) -> Result<(), Box<dyn std::error::Error>> {
    let customer_id = resolve_customer_id(target)?;
    let client = client_for_target(target).await?;
    let query = format!(
        "SELECT campaign.id, campaign.name, ad_group.id, ad_group.name, ad_group_ad.ad.id, ad_group_ad.status, ad_group_ad.ad.type \
         FROM ad_group_ad \
         WHERE ad_group_ad.status != 'REMOVED' \
         ORDER BY ad_group_ad.ad.id DESC \
         LIMIT {limit}"
    );

    run_gaql_query(&client, &customer_id, &query).await
}

async fn run_creatives(target: &TargetArgs, limit: u32) -> Result<(), Box<dyn std::error::Error>> {
    let customer_id = resolve_customer_id(target)?;
    let client = client_for_target(target).await?;
    let query = format!(
        "SELECT campaign.id, campaign.name, ad_group.id, ad_group.name, ad_group_ad.ad.id, ad_group_ad.ad.type, ad_group_ad.status, \
         ad_group_ad.ad.final_urls, ad_group_ad.ad.responsive_search_ad.path1, ad_group_ad.ad.responsive_search_ad.path2, \
         ad_group_ad.ad.responsive_search_ad.headlines, ad_group_ad.ad.responsive_search_ad.descriptions \
         FROM ad_group_ad \
         WHERE ad_group_ad.status != 'REMOVED' \
         ORDER BY ad_group_ad.ad.id DESC \
         LIMIT {limit}"
    );

    run_gaql_query(&client, &customer_id, &query).await
}

async fn run_campaign_status(
    target: &TargetArgs,
    campaign_id: &str,
    status: DeliveryStatus,
    validate_only: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let customer_id = resolve_customer_id(target)?;
    let client = client_for_target(target).await?;
    let resource_name = format!("customers/{customer_id}/campaigns/{campaign_id}");
    let payload = json!({
        "operations": [{
            "update": {
                "resourceName": resource_name,
                "status": status.as_api_status(),
            },
            "updateMask": "status"
        }],
        "validateOnly": validate_only
    });

    run_mutate_request(&client, &customer_id, "campaigns", payload).await
}

async fn run_ad_group_status(
    target: &TargetArgs,
    ad_group_id: &str,
    status: DeliveryStatus,
    validate_only: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let customer_id = resolve_customer_id(target)?;
    let client = client_for_target(target).await?;
    let resource_name = format!("customers/{customer_id}/adGroups/{ad_group_id}");
    let payload = json!({
        "operations": [{
            "update": {
                "resourceName": resource_name,
                "status": status.as_api_status(),
            },
            "updateMask": "status"
        }],
        "validateOnly": validate_only
    });

    run_mutate_request(&client, &customer_id, "adGroups", payload).await
}

async fn run_ad_status(
    target: &TargetArgs,
    ad_group_id: &str,
    ad_id: &str,
    status: DeliveryStatus,
    validate_only: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let customer_id = resolve_customer_id(target)?;
    let client = client_for_target(target).await?;
    let resource_name = format!("customers/{customer_id}/adGroupAds/{ad_group_id}~{ad_id}");
    let payload = json!({
        "operations": [{
            "update": {
                "resourceName": resource_name,
                "status": status.as_api_status(),
            },
            "updateMask": "status"
        }],
        "validateOnly": validate_only
    });

    run_mutate_request(&client, &customer_id, "adGroupAds", payload).await
}

async fn run_mutate(
    target: &TargetArgs,
    service: &str,
    body: Option<String>,
    body_file: Option<PathBuf>,
    validate_only: bool,
    partial_failure: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let customer_id = resolve_customer_id(target)?;
    let client = client_for_target(target).await?;
    let mut payload = parse_mutate_payload(body, body_file)?;
    let obj = payload
        .as_object_mut()
        .ok_or("mutate body must be a JSON object")?;

    if validate_only {
        obj.insert("validateOnly".to_owned(), Value::Bool(true));
    }
    if partial_failure {
        obj.insert("partialFailure".to_owned(), Value::Bool(true));
    }

    run_mutate_request(&client, &customer_id, service, payload).await
}

async fn run_mutate_request(
    client: &GoogleAdsClient,
    customer_id: &str,
    service: &str,
    payload: Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let response = client.mutate(customer_id, service, payload).await?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

async fn run_gaql_query(
    client: &GoogleAdsClient,
    customer_id: &str,
    query: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let response = client.search_stream(customer_id, query).await?;
    let rows = format::flatten_search_response(&response)?;

    if rows.is_empty() {
        println!("No results found.");
    } else {
        // Output as JSON array for CLI consumption
        println!("{}", serde_json::to_string_pretty(&rows)?);
    }

    Ok(())
}

fn resolve_customer_id(target: &TargetArgs) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(customer_id) = target.customer_id.clone() {
        return Ok(customer_id);
    }

    if let Some(saved_customer_id) = profile::load_customer_id()? {
        return Ok(saved_customer_id);
    }

    Err("customer ID is required. Pass --customer-id or run `gads use <CUSTOMER_ID>`.".into())
}

fn login_customer_id_override(target: &TargetArgs) -> Option<Option<String>> {
    if target.no_login_customer_id {
        Some(None)
    } else {
        target.login_customer_id.clone().map(Some)
    }
}

async fn client_for_target(
    target: &TargetArgs,
) -> Result<GoogleAdsClient, Box<dyn std::error::Error>> {
    let override_value = login_customer_id_override(target);
    Ok(GoogleAdsClient::new_with_login_customer_id_override(override_value).await?)
}

fn parse_mutate_payload(
    body: Option<String>,
    body_file: Option<PathBuf>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let raw = match (body, body_file) {
        (Some(body), None) => body,
        (None, Some(path)) => std::fs::read_to_string(&path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?,
        _ => return Err("either --body or --body-file must be provided (and not both)".into()),
    };

    let value: Value =
        serde_json::from_str(&raw).map_err(|err| format!("invalid JSON mutate body: {err}"))?;

    if !value.is_object() {
        return Err("mutate body must be a JSON object".into());
    }

    Ok(value)
}

fn parse_resource_id_arg(value: &str) -> Result<String, String> {
    let trimmed = value.trim();

    if trimmed.is_empty() || !trimmed.chars().all(|c| c.is_ascii_digit()) {
        return Err("must be digits only (no hyphens), e.g. 1234567890".to_owned());
    }

    Ok(trimmed.to_owned())
}

fn parse_mutate_service_arg(value: &str) -> Result<String, String> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        return Err("service must not be empty".to_owned());
    }

    let mut chars = trimmed.chars();
    let starts_with_letter = chars
        .next()
        .map(|c| c.is_ascii_alphabetic())
        .unwrap_or(false);

    if !starts_with_letter || !chars.all(|c| c.is_ascii_alphanumeric()) {
        return Err(
            "service must be an API service name like campaigns, adGroups, adGroupAds".to_owned(),
        );
    }

    Ok(trimmed.to_owned())
}
