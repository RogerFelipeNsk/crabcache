//! Security module for CrabCache

pub mod auth;
pub mod rate_limit;
pub mod ip_filter;

pub use auth::{AuthManager, AuthResult};
pub use rate_limit::{RateLimiter, RateLimitResult};
pub use ip_filter::{IpFilter, IpFilterResult};

use std::net::IpAddr;

/// Security context for a connection
#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub client_ip: IpAddr,
    pub authenticated: bool,
    pub user_id: Option<String>,
    pub rate_limit_key: String,
}

impl SecurityContext {
    pub fn new(client_ip: IpAddr) -> Self {
        Self {
            client_ip,
            authenticated: false,
            user_id: None,
            rate_limit_key: client_ip.to_string(),
        }
    }
    
    pub fn authenticate(&mut self, user_id: String) {
        self.authenticated = true;
        self.user_id = Some(user_id);
    }
}

/// Security manager that combines all security features
pub struct SecurityManager {
    auth_manager: Option<AuthManager>,
    rate_limiter: Option<RateLimiter>,
    ip_filter: Option<IpFilter>,
}

impl SecurityManager {
    pub fn new(
        auth_manager: Option<AuthManager>,
        rate_limiter: Option<RateLimiter>,
        ip_filter: Option<IpFilter>,
    ) -> Self {
        Self {
            auth_manager,
            rate_limiter,
            ip_filter,
        }
    }
    
    /// Check if connection is allowed
    pub async fn check_connection(&self, context: &SecurityContext) -> SecurityCheckResult {
        // Check IP filter first
        if let Some(ref ip_filter) = self.ip_filter {
            match ip_filter.check_ip(context.client_ip) {
                IpFilterResult::Allowed => {}
                IpFilterResult::Blocked => {
                    return SecurityCheckResult::IpBlocked;
                }
            }
        }
        
        // Check rate limiting
        if let Some(ref rate_limiter) = self.rate_limiter {
            match rate_limiter.check_rate(&context.rate_limit_key).await {
                RateLimitResult::Allowed => {}
                RateLimitResult::RateLimited => {
                    return SecurityCheckResult::RateLimited;
                }
            }
        }
        
        SecurityCheckResult::Allowed
    }
    
    /// Authenticate a command
    pub fn authenticate_command(&self, context: &mut SecurityContext, auth_token: Option<&str>) -> SecurityCheckResult {
        if let Some(ref auth_manager) = self.auth_manager {
            match auth_manager.authenticate(auth_token) {
                AuthResult::Authenticated(user_id) => {
                    context.authenticate(user_id);
                    SecurityCheckResult::Allowed
                }
                AuthResult::Unauthenticated => SecurityCheckResult::AuthRequired,
                AuthResult::InvalidToken => SecurityCheckResult::AuthFailed,
            }
        } else {
            // No authentication required
            SecurityCheckResult::Allowed
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SecurityCheckResult {
    Allowed,
    IpBlocked,
    RateLimited,
    AuthRequired,
    AuthFailed,
}

impl SecurityCheckResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, SecurityCheckResult::Allowed)
    }
    
    pub fn error_message(&self) -> &'static str {
        match self {
            SecurityCheckResult::Allowed => "OK",
            SecurityCheckResult::IpBlocked => "IP address not allowed",
            SecurityCheckResult::RateLimited => "Rate limit exceeded",
            SecurityCheckResult::AuthRequired => "Authentication required",
            SecurityCheckResult::AuthFailed => "Authentication failed",
        }
    }
}