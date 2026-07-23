//! Authentication module: Token, Basic Auth, and interactive password input.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use std::io::{self, Write};

/// Supported authentication methods
#[derive(Debug, Clone)]
pub enum AuthMethod {
    /// Service Account Token (Bearer)
    Bearer(String),
    /// Basic HTTP Authentication
    Basic(String, String),
}

impl AuthMethod {
    /// Build authorization header value (e.g. "Bearer glsa_xxx" or "Basic base64string")
    pub fn header_value(&self) -> String {
        match self {
            AuthMethod::Bearer(token) => format!("Bearer {token}"),
            AuthMethod::Basic(user, pass) => {
                let credentials = format!("{user}:{pass}");
                format!("Basic {}", BASE64.encode(credentials))
            }
        }
    }

    /// Apply auth to a reqwest RequestBuilder
    #[allow(dead_code)]
    pub fn apply_to_request(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        builder.header("Authorization", self.header_value())
    }

    /// Apply auth to a tokio-tungstenite http::request::Builder
    #[allow(dead_code)]
    pub fn apply_to_ws_builder(
        &self,
        builder: http::request::Builder,
    ) -> http::request::Builder {
        builder.header("Authorization", self.header_value())
    }
}

/// Resolve authentication from CLI args, environment, or interactive prompt.
///
/// Priority: Token > Basic Auth (user + password) > Interactive prompt
pub fn resolve_auth(
    token: Option<&str>,
    user: Option<&str>,
    password: Option<&str>,
) -> crate::error::Result<AuthMethod> {
    // Priority 1: Service Account Token
    if let Some(t) = token {
        if !t.is_empty() {
            return Ok(AuthMethod::Bearer(t.to_string()));
        }
    }

    // Priority 2: Basic Auth with user + password
    if let Some(u) = user {
        if let Some(p) = password {
            if !p.is_empty() {
                return Ok(AuthMethod::Basic(u.to_string(), p.to_string()));
            }
        }
    }

    // Priority 3: User provided but no password — interactive prompt
    if let Some(u) = user {
        let pass = prompt_password(u)?;
        return Ok(AuthMethod::Basic(u.to_string(), pass));
    }

    Err(crate::error::GraftailError::Auth(
        "No authentication provided. Set --token, GRAFTAIL_TOKEN, or --user/--password.".into(),
    ))
}

/// Prompt the user for a password (hidden input)
fn prompt_password(username: &str) -> crate::error::Result<String> {
    print!("Password for {username}: ");
    io::stdout().flush().map_err(|e| crate::error::GraftailError::Io(e))?;
    let password = rpassword::read_password().map_err(|e| {
        crate::error::GraftailError::Auth(format!("Failed to read password: {e}"))
    })?;
    Ok(password)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bearer_token_header() {
        let auth = AuthMethod::Bearer("glsa_test".into());
        assert_eq!(auth.header_value(), "Bearer glsa_test");
    }

    #[test]
    fn test_basic_auth_header() {
        let auth = AuthMethod::Basic("admin".into(), "secret".into());
        let val = auth.header_value();
        assert!(val.starts_with("Basic "));
        // Decode and verify
        let encoded = val.strip_prefix("Basic ").unwrap();
        let decoded = String::from_utf8(BASE64.decode(encoded).unwrap()).unwrap();
        assert_eq!(decoded, "admin:secret");
    }

    #[test]
    fn test_resolve_auth_token_priority() {
        let auth = resolve_auth(Some("tok123"), Some("user"), Some("pass")).unwrap();
        assert!(matches!(auth, AuthMethod::Bearer(t) if t == "tok123"));
    }

    #[test]
    fn test_resolve_auth_basic() {
        let auth = resolve_auth(None, Some("user"), Some("pass")).unwrap();
        assert!(matches!(auth, AuthMethod::Basic(u, p) if u == "user" && p == "pass"));
    }

    #[test]
    fn test_resolve_auth_no_token_empty_basic() {
        // Token is empty string, basic auth has empty password
        let auth = resolve_auth(Some(""), Some("user"), Some("")).unwrap_err();
        assert!(matches!(auth, crate::error::GraftailError::Auth(_)));
    }

    #[test]
    fn test_resolve_auth_none() {
        let err = resolve_auth(None, None, None).unwrap_err();
        assert!(matches!(err, crate::error::GraftailError::Auth(_)));
    }
}
