use std::fmt::Write;

use crate::error::GoogleAdsError;

pub struct GaqlQuery<'a> {
    pub fields: &'a [String],
    pub resource: &'a str,
    pub conditions: Option<&'a [String]>,
    pub orderings: Option<&'a [String]>,
    pub limit: Option<u32>,
}

impl GaqlQuery<'_> {
    pub fn build(&self) -> Result<String, GoogleAdsError> {
        if self.fields.is_empty() {
            return Err(GoogleAdsError::QueryBuild(
                "at least one field is required".into(),
            ));
        }
        if self.resource.is_empty() {
            return Err(GoogleAdsError::QueryBuild("resource is required".into()));
        }

        let mut query = format!("SELECT {} FROM {}", self.fields.join(", "), self.resource);

        if let Some(conditions) = self.conditions {
            if !conditions.is_empty() {
                let _ = write!(query, " WHERE {}", conditions.join(" AND "));
            }
        }

        if let Some(orderings) = self.orderings {
            if !orderings.is_empty() {
                let _ = write!(query, " ORDER BY {}", orderings.join(", "));
            }
        }

        if let Some(limit) = self.limit {
            let _ = write!(query, " LIMIT {limit}");
        }

        Ok(query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_select() {
        let fields = vec!["campaign.id".into(), "campaign.name".into()];
        let q = GaqlQuery {
            fields: &fields,
            resource: "campaign",
            conditions: None,
            orderings: None,
            limit: None,
        };
        assert_eq!(
            q.build().unwrap(),
            "SELECT campaign.id, campaign.name FROM campaign"
        );
    }

    #[test]
    fn with_conditions() {
        let fields = vec!["campaign.id".into()];
        let conditions = vec![
            "campaign.status = 'ENABLED'".into(),
            "metrics.clicks > 0".into(),
        ];
        let q = GaqlQuery {
            fields: &fields,
            resource: "campaign",
            conditions: Some(&conditions),
            orderings: None,
            limit: None,
        };
        assert_eq!(
            q.build().unwrap(),
            "SELECT campaign.id FROM campaign WHERE campaign.status = 'ENABLED' AND metrics.clicks > 0"
        );
    }

    #[test]
    fn with_orderings() {
        let fields = vec!["campaign.id".into(), "metrics.clicks".into()];
        let orderings = vec!["metrics.clicks DESC".into()];
        let q = GaqlQuery {
            fields: &fields,
            resource: "campaign",
            conditions: None,
            orderings: Some(&orderings),
            limit: None,
        };
        assert_eq!(
            q.build().unwrap(),
            "SELECT campaign.id, metrics.clicks FROM campaign ORDER BY metrics.clicks DESC"
        );
    }

    #[test]
    fn with_limit() {
        let fields = vec!["campaign.id".into()];
        let q = GaqlQuery {
            fields: &fields,
            resource: "campaign",
            conditions: None,
            orderings: None,
            limit: Some(100),
        };
        assert_eq!(
            q.build().unwrap(),
            "SELECT campaign.id FROM campaign LIMIT 100"
        );
    }

    #[test]
    fn full_query() {
        let fields = vec![
            "campaign.id".into(),
            "campaign.name".into(),
            "metrics.clicks".into(),
        ];
        let conditions = vec!["campaign.status = 'ENABLED'".into()];
        let orderings = vec!["metrics.clicks DESC".into()];
        let q = GaqlQuery {
            fields: &fields,
            resource: "campaign",
            conditions: Some(&conditions),
            orderings: Some(&orderings),
            limit: Some(50),
        };
        assert_eq!(
            q.build().unwrap(),
            "SELECT campaign.id, campaign.name, metrics.clicks FROM campaign WHERE campaign.status = 'ENABLED' ORDER BY metrics.clicks DESC LIMIT 50"
        );
    }

    #[test]
    fn empty_fields_error() {
        let fields: Vec<String> = vec![];
        let q = GaqlQuery {
            fields: &fields,
            resource: "campaign",
            conditions: None,
            orderings: None,
            limit: None,
        };
        let err = q.build().unwrap_err();
        assert!(err.to_string().contains("at least one field"));
    }

    #[test]
    fn empty_resource_error() {
        let fields = vec!["campaign.id".into()];
        let q = GaqlQuery {
            fields: &fields,
            resource: "",
            conditions: None,
            orderings: None,
            limit: None,
        };
        let err = q.build().unwrap_err();
        assert!(err.to_string().contains("resource is required"));
    }

    #[test]
    fn empty_conditions_ignored() {
        let fields = vec!["campaign.id".into()];
        let conditions: Vec<String> = vec![];
        let q = GaqlQuery {
            fields: &fields,
            resource: "campaign",
            conditions: Some(&conditions),
            orderings: None,
            limit: None,
        };
        assert_eq!(q.build().unwrap(), "SELECT campaign.id FROM campaign");
    }
}
