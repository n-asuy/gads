use serde_json::Value;

use crate::error::GoogleAdsError;

/// Extract and flatten results from a `searchStream` REST response.
///
/// The response is a JSON array of batches:
/// ```json
/// [
///   {
///     "results": [ { "campaign": { "name": "..." }, "metrics": { ... } }, ... ],
///     "fieldMask": "campaign.name,metrics.clicks"
///   }
/// ]
/// ```
///
/// Each row is flattened using the `field_mask` paths into `{ "campaign.name": "...", "metrics.clicks": 42 }`.
pub fn flatten_search_response(response: &Value) -> Result<Vec<Value>, GoogleAdsError> {
    let batches = response.as_array().ok_or_else(|| {
        GoogleAdsError::InvalidResponse("searchStream response is not an array".into())
    })?;

    let mut rows = Vec::new();

    for batch in batches {
        let field_paths: Vec<&str> = batch["fieldMask"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.split(',').collect())
            .unwrap_or_default();

        if let Some(results) = batch["results"].as_array() {
            for row in results {
                let mut flat = serde_json::Map::new();
                for path in &field_paths {
                    flat.insert(path.to_string(), resolve_path(row, path));
                }
                rows.push(Value::Object(flat));
            }
        }
    }

    Ok(rows)
}

/// Resolve a dotted path like `"campaign.name"` against nested JSON.
fn resolve_path(value: &Value, path: &str) -> Value {
    let mut current = value;
    for segment in path.split('.') {
        match current.get(segment) {
            Some(v) => current = v,
            None => return Value::Null,
        }
    }
    current.clone()
}

/// Format flattened rows as human-readable text for LLM consumption.
pub fn format_rows_as_text(rows: &[Value]) -> String {
    if rows.is_empty() {
        return "No results found.".to_string();
    }

    let mut output = String::new();
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        if let Value::Object(map) = row {
            let pairs: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{k}: {}", format_value(v)))
                .collect();
            output.push_str(&pairs.join(", "));
        }
    }
    output
}

fn format_value(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flatten_single_batch() {
        let response = json!([
            {
                "results": [
                    { "campaign": { "id": "123", "name": "Test Campaign" } }
                ],
                "fieldMask": "campaign.id,campaign.name"
            }
        ]);
        let rows = flatten_search_response(&response).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["campaign.id"], json!("123"));
        assert_eq!(rows[0]["campaign.name"], json!("Test Campaign"));
    }

    #[test]
    fn flatten_multiple_batches() {
        let response = json!([
            {
                "results": [
                    { "campaign": { "id": "1" } }
                ],
                "fieldMask": "campaign.id"
            },
            {
                "results": [
                    { "campaign": { "id": "2" } },
                    { "campaign": { "id": "3" } }
                ],
                "fieldMask": "campaign.id"
            }
        ]);
        let rows = flatten_search_response(&response).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["campaign.id"], json!("1"));
        assert_eq!(rows[1]["campaign.id"], json!("2"));
        assert_eq!(rows[2]["campaign.id"], json!("3"));
    }

    #[test]
    fn flatten_empty_results() {
        let response = json!([
            {
                "results": [],
                "fieldMask": "campaign.id"
            }
        ]);
        let rows = flatten_search_response(&response).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn flatten_no_results_key() {
        let response = json!([
            {
                "fieldMask": "campaign.id"
            }
        ]);
        let rows = flatten_search_response(&response).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn not_array_error() {
        let response = json!({ "error": "bad" });
        let err = flatten_search_response(&response).unwrap_err();
        assert!(err.to_string().contains("not an array"));
    }

    #[test]
    fn resolve_nested_path() {
        let row = json!({ "campaign": { "name": "Hello" } });
        assert_eq!(resolve_path(&row, "campaign.name"), json!("Hello"));
    }

    #[test]
    fn resolve_missing_path() {
        let row = json!({ "campaign": { "name": "Hello" } });
        assert_eq!(resolve_path(&row, "campaign.missing"), Value::Null);
    }

    #[test]
    fn resolve_deeply_nested() {
        let row = json!({ "ad_group": { "criterion": { "keyword": { "text": "buy" } } } });
        assert_eq!(
            resolve_path(&row, "ad_group.criterion.keyword.text"),
            json!("buy")
        );
    }

    #[test]
    fn format_text_basic() {
        let rows = vec![
            json!({"campaign.id": "123", "campaign.name": "Test"}),
            json!({"campaign.id": "456", "campaign.name": "Other"}),
        ];
        let text = format_rows_as_text(&rows);
        assert!(text.contains("campaign.id: 123"));
        assert!(text.contains("campaign.name: Test"));
        assert!(text.contains("campaign.id: 456"));
        assert!(text.contains('\n'));
    }

    #[test]
    fn format_text_empty() {
        let rows: Vec<Value> = vec![];
        assert_eq!(format_rows_as_text(&rows), "No results found.");
    }

    #[test]
    fn format_value_types() {
        assert_eq!(format_value(&json!("hello")), "hello");
        assert_eq!(format_value(&json!(42)), "42");
        assert_eq!(format_value(&json!(true)), "true");
        assert_eq!(format_value(&Value::Null), "null");
        assert_eq!(format_value(&json!({"a": 1})), r#"{"a":1}"#);
    }
}
