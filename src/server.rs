use std::sync::Arc;

use rmcp::handler::server::router::tool::{ToolRoute, ToolRouter};
use rmcp::model::{CallToolResult, Content, ServerCapabilities, ServerInfo, Tool};
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::client::GoogleAdsClient;
use crate::format;
use crate::ids;
use crate::query::GaqlQuery;

const GAQL_RESOURCES: &str = include_str!("gaql_resources.json");

fn search_description() -> String {
    format!(
        r"Fetches data from the Google Ads API using GAQL (Google Ads Query Language).

### Hints
- Language Grammar: https://developers.google.com/google-ads/api/docs/query/grammar
- All resources and descriptions: https://developers.google.com/google-ads/api/fields/v21/overview
- For Conversion issues try looking in offline_conversion_upload_conversion_action_summary

### Hint for customer_id
- Should be a string of numbers without punctuation
- If presented as 123-456-7890, remove hyphens and use 1234567890

### Hints for Dates
- All dates must be in YYYY-MM-DD format with dashes
- Date literals from the Grammar must NEVER be used
- Date ranges must be finite with start and end dates

### Hints for limits
- Requests to resource change_event must specify a LIMIT <= 10000

### Hints for conversions questions
- https://developers.google.com/google-ads/api/docs/conversions/upload-summaries

### Available fields
What follows is a table of resources and their selectable fields (fields), filterable fields (used in the condition) and sortable fields (use in the ordering).
Fields are comma separated, the whole field must be used, wildcards and partial fields are not allowed.
All fields must come from this table and be prefixed with the resource being searched.
{GAQL_RESOURCES}"
    )
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// The customer ID (numbers only, no hyphens). Example: "1234567890"
    pub customer_id: String,
    /// The fields to fetch. Must be fully qualified field names from the resources table.
    pub fields: Vec<String>,
    /// The resource to query. Example: `campaign`, `ad_group`, `ad_group_ad`
    pub resource: String,
    /// Conditions to filter data, combined using AND clauses.
    #[serde(default)]
    pub conditions: Option<Vec<String>>,
    /// Ordering specification. Example: `metrics.clicks DESC`
    #[serde(default)]
    pub orderings: Option<Vec<String>>,
    /// Maximum number of rows to return.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Clone)]
pub struct GadsServer {
    client: Arc<GoogleAdsClient>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl GadsServer {
    pub async fn new() -> Result<Self, crate::error::GoogleAdsError> {
        let client = GoogleAdsClient::new().await?;
        let mut router = Self::tool_router();

        // Register the search tool with a dynamic description containing GAQL field reference.
        let search_schema = schemars::schema_for!(SearchParams);
        let schema_obj = match search_schema.to_value() {
            serde_json::Value::Object(obj) => obj,
            _ => serde_json::Map::new(),
        };

        let client_arc = Arc::new(client);
        let client_for_search = Arc::clone(&client_arc);

        router.add_route(ToolRoute::new_dyn(
            Tool::new("search", search_description(), Arc::new(schema_obj)),
            move |context| {
                let client = Arc::clone(&client_for_search);
                Box::pin(async move { handle_search(&client, context.arguments).await })
            },
        ));

        Ok(Self {
            client: client_arc,
            tool_router: router,
        })
    }

    #[tool(
        description = "Returns IDs of customers directly accessible by the user authenticating the call."
    )]
    async fn list_accessible_customers(&self) -> Result<CallToolResult, ErrorData> {
        let customer_ids = self
            .client
            .list_accessible_customers()
            .await
            .map_err(ErrorData::from)?;

        let output = if customer_ids.is_empty() {
            "No accessible customers found.".to_string()
        } else {
            customer_ids.join("\n")
        };

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}

async fn handle_search(
    client: &GoogleAdsClient,
    arguments: Option<serde_json::Map<String, serde_json::Value>>,
) -> Result<CallToolResult, ErrorData> {
    let args =
        arguments.ok_or_else(|| ErrorData::invalid_params("search requires arguments", None))?;

    let params: SearchParams = serde_json::from_value(serde_json::Value::Object(args))
        .map_err(|e| ErrorData::invalid_params(format!("invalid search parameters: {e}"), None))?;

    let query = GaqlQuery {
        fields: &params.fields,
        resource: &params.resource,
        conditions: params.conditions.as_deref(),
        orderings: params.orderings.as_deref(),
        limit: params.limit,
    };

    let gaql = query.build().map_err(ErrorData::from)?;
    tracing::info!(query = %gaql, "executing GAQL search");
    let customer_id =
        ids::normalize_customer_id(&params.customer_id, "customer_id").map_err(ErrorData::from)?;

    let response = client
        .search_stream(&customer_id, &gaql)
        .await
        .map_err(ErrorData::from)?;

    let rows = format::flatten_search_response(&response).map_err(ErrorData::from)?;
    let output = format::format_rows_as_text(&rows);

    Ok(CallToolResult::success(vec![Content::text(output)]))
}

#[tool_handler]
impl ServerHandler for GadsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Google Ads MCP Server. Query Google Ads data using GAQL and list accessible customer accounts.".into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
