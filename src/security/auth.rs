//! Authentication module for CrabCache

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Authentication manager
pub struct AuthManager {
    /// Valid tokens and their associated user IDs
    tokens: Arc<RwLock<HashMap<String, String>>>,
    /// Whether authentication is required
    required: bool,
}

impl AuthManager {
    /// Create new authentication manager
    pub fn new(required: bool) -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
            required,
        }
    }
    
    /// Create authentication manager with a single token
    pub fn with_token(token: String, user_id: String) -> Self {
        let mut tokens = HashMap::new();
        tokens.insert(token, user_id);
        
        Self {
            tokens: Arc::new(RwLock::new(tokens)),
            required: true,
        }
    }
    
    /// Add a new authentication token
    pub async fn add_token(&self, token: String, user_id: String) {
        let mut tokens = self.tokens.write().await;
        debug!("Added authentication token for user: {}", user_id);
        tokens.insert(token, user_id);
    }
    
    /// Remove an authentication token
    pub async fn remove_token(&self, token: &str) -> bool {
        let mut tokens = self.tokens.write().await;
        if let Some(user_id) = tokens.remove(token) {
            debug!("Removed authentication token for user: {}", user_id);
            true
        } else {
            false
        }
    }
    
    /// Authenticate a token
    pub fn authenticate(&self, token: Option<&str>) -> AuthResult {
        if !self.required {
            return AuthResult::Authenticated("anonymous".to_string());
        }
        
        match token {
            Some(token) => {
                // Note: This is a blocking operation, but it's very fast
                // In a real implementation, you might want to use async here
                let tokens = match self.tokens.try_read() {
                    Ok(tokens) => tokens,
                    Err(_) => {
                        warn!("Failed to acquire read lock for tokens");
                        return AuthResult::InvalidToken;
                    }
                };
                
                if let Some(user_id) = tokens.get(token) {
                    debug!("Authentication successful for user: {}", user_id);
                    AuthResult::Authenticated(user_id.clone())
                } else {
                    warn!("Authentication failed: invalid token");
                    AuthResult::InvalidToken
                }
            }
            None => {
                debug!("Authentication required but no token provided");
                AuthResult::Unauthenticated
            }
        }
    }
    
    /// Check if authentication is required
    pub fn is_required(&self) -> bool {
        self.required
    }
    
    /// Get number of active tokens
    pub async fn token_count(&self) -> usize {
        let tokens = self.tokens.read().await;
        tokens.len()
    }
    
    /// List all user IDs (for admin purposes)
    pub async fn list_users(&self) -> Vec<String> {
        let tokens = self.tokens.read().await;
        tokens.values().cloned().collect()
    }
}

/// Authentication result
#[derive(Debug, Clone, PartialEq)]
pub enum AuthResult {
    /// Authentication successful with user ID
    Authenticated(String),
    /// No token provided but authentication required
    Unauthenticated,
    /// Invalid token provided
    InvalidToken,
}

impl AuthResult {
    pub fn is_authenticated(&self) -> bool {
        matches!(self, AuthResult::Authenticated(_))
    }
    
    pub fn user_id(&self) -> Option<&str> {
        match self {
            AuthResult::Authenticated(user_id) => Some(user_id),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_auth_manager_no_auth_required() {
        let auth_manager = AuthManager::new(false);
        
        // Should allow any request when auth is not required
        let result = auth_manager.authenticate(None);
        assert_eq!(result, AuthResult::Authenticated("anonymous".to_string()));
        
        let result = auth_manager.authenticate(Some("invalid_token"));
        assert_eq!(result, AuthResult::Authenticated("anonymous".to_string()));
    }
    
    #[tokio::test]
    async fn test_auth_manager_with_token() {
        let auth_manager = AuthManager::with_token("secret123".to_string(), "user1".to_string());
        
        // Valid token should authenticate
        let result = auth_manager.authenticate(Some("secret123"));
        assert_eq!(result, AuthResult::Authenticated("user1".to_string()));
        
        // Invalid token should fail
        let result = auth_manager.authenticate(Some("invalid"));
        assert_eq!(result, AuthResult::InvalidToken);
        
        // No token should require auth
        let result = auth_manager.authenticate(None);
        assert_eq!(result, AuthResult::Unauthenticated);
    }
    
    #[tokio::test]
    async fn test_auth_manager_add_remove_tokens() {
        let auth_manager = AuthManager::new(true);
        
        // Add token
        auth_manager.add_token("token1".to_string(), "user1".to_string()).await;
        let result = auth_manager.authenticate(Some("token1"));
        assert_eq!(result, AuthResult::Authenticated("user1".to_string()));
        
        // Remove token
        let removed = auth_manager.remove_token("token1").await;
        assert!(removed);
        
        let result = auth_manager.authenticate(Some("token1"));
        assert_eq!(result, AuthResult::InvalidToken);
    }
}