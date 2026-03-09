use crate::error::GoogleAdsError;

const CUSTOMER_ID_HELP: &str = "must be digits only (no hyphens), e.g. 1234567890";

fn normalize_customer_id_impl(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("customer ID {CUSTOMER_ID_HELP}"));
    }

    if !trimmed.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("customer ID {CUSTOMER_ID_HELP}"));
    }

    Ok(trimmed.to_owned())
}

pub fn parse_customer_id_arg(value: &str) -> Result<String, String> {
    normalize_customer_id_impl(value)
}

pub fn normalize_customer_id(value: &str, field: &'static str) -> Result<String, GoogleAdsError> {
    normalize_customer_id_impl(value).map_err(|_| GoogleAdsError::InvalidCustomerId {
        field,
        value: value.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::normalize_customer_id_impl;

    #[test]
    fn validates_digits_only() {
        assert_eq!(
            normalize_customer_id_impl("1234567890").unwrap(),
            "1234567890"
        );
        assert!(normalize_customer_id_impl("123-456-7890").is_err());
        assert!(normalize_customer_id_impl(" ").is_err());
        assert!(normalize_customer_id_impl("abc").is_err());
    }
}
