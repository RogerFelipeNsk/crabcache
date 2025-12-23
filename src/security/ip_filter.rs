//! IP filtering module for CrabCache

use std::collections::HashSet;
use std::net::IpAddr;
use std::str::FromStr;
use tracing::{debug, warn};

/// IP address filter
pub struct IpFilter {
    /// Allowed IP addresses (empty = allow all)
    allowed_ips: HashSet<IpAddr>,
    /// Allowed IP networks (CIDR notation)
    allowed_networks: Vec<IpNetwork>,
    /// Whether to allow all IPs when no specific IPs are configured
    allow_all_when_empty: bool,
}

impl IpFilter {
    /// Create new IP filter
    pub fn new(allowed_ips: Vec<String>, allow_all_when_empty: bool) -> Result<Self, String> {
        let mut filter = Self {
            allowed_ips: HashSet::new(),
            allowed_networks: Vec::new(),
            allow_all_when_empty,
        };
        
        for ip_str in allowed_ips {
            filter.add_allowed_ip(&ip_str)?;
        }
        
        Ok(filter)
    }
    
    /// Add an allowed IP address or network
    pub fn add_allowed_ip(&mut self, ip_str: &str) -> Result<(), String> {
        let ip_str = ip_str.trim();
        
        if ip_str.contains('/') {
            // CIDR notation
            let network = IpNetwork::from_str(ip_str)
                .map_err(|e| format!("Invalid CIDR notation '{}': {}", ip_str, e))?;
            self.allowed_networks.push(network);
            debug!("Added allowed network: {}", ip_str);
        } else {
            // Single IP address
            let ip = IpAddr::from_str(ip_str)
                .map_err(|e| format!("Invalid IP address '{}': {}", ip_str, e))?;
            self.allowed_ips.insert(ip);
            debug!("Added allowed IP: {}", ip);
        }
        
        Ok(())
    }
    
    /// Check if an IP address is allowed
    pub fn check_ip(&self, ip: IpAddr) -> IpFilterResult {
        // If no IPs are configured and allow_all_when_empty is true, allow all
        if self.allowed_ips.is_empty() && self.allowed_networks.is_empty() && self.allow_all_when_empty {
            return IpFilterResult::Allowed;
        }
        
        // Check exact IP matches
        if self.allowed_ips.contains(&ip) {
            debug!("IP {} allowed (exact match)", ip);
            return IpFilterResult::Allowed;
        }
        
        // Check network matches
        for network in &self.allowed_networks {
            if network.contains(ip) {
                debug!("IP {} allowed (network match: {})", ip, network);
                return IpFilterResult::Allowed;
            }
        }
        
        warn!("IP {} blocked (not in allowed list)", ip);
        IpFilterResult::Blocked
    }
    
    /// Get number of allowed IPs
    pub fn allowed_ip_count(&self) -> usize {
        self.allowed_ips.len()
    }
    
    /// Get number of allowed networks
    pub fn allowed_network_count(&self) -> usize {
        self.allowed_networks.len()
    }
    
    /// Check if filter allows all IPs
    pub fn allows_all(&self) -> bool {
        self.allowed_ips.is_empty() && self.allowed_networks.is_empty() && self.allow_all_when_empty
    }
}

/// IP network representation for CIDR notation
#[derive(Debug, Clone)]
pub struct IpNetwork {
    network: IpAddr,
    prefix_len: u8,
}

impl IpNetwork {
    pub fn new(network: IpAddr, prefix_len: u8) -> Result<Self, String> {
        match network {
            IpAddr::V4(_) => {
                if prefix_len > 32 {
                    return Err("IPv4 prefix length cannot exceed 32".to_string());
                }
            }
            IpAddr::V6(_) => {
                if prefix_len > 128 {
                    return Err("IPv6 prefix length cannot exceed 128".to_string());
                }
            }
        }
        
        Ok(Self { network, prefix_len })
    }
    
    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.network, ip) {
            (IpAddr::V4(net), IpAddr::V4(addr)) => {
                let net_bits = u32::from(net);
                let addr_bits = u32::from(addr);
                let mask = !((1u32 << (32 - self.prefix_len)) - 1);
                (net_bits & mask) == (addr_bits & mask)
            }
            (IpAddr::V6(net), IpAddr::V6(addr)) => {
                let net_bits = u128::from(net);
                let addr_bits = u128::from(addr);
                let mask = !((1u128 << (128 - self.prefix_len)) - 1);
                (net_bits & mask) == (addr_bits & mask)
            }
            _ => false, // IPv4 vs IPv6 mismatch
        }
    }
}

impl FromStr for IpNetwork {
    type Err = String;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 2 {
            return Err("CIDR notation must contain exactly one '/'".to_string());
        }
        
        let network = IpAddr::from_str(parts[0])
            .map_err(|e| format!("Invalid network address: {}", e))?;
        let prefix_len = parts[1].parse::<u8>()
            .map_err(|e| format!("Invalid prefix length: {}", e))?;
        
        Self::new(network, prefix_len)
    }
}

impl std::fmt::Display for IpNetwork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.network, self.prefix_len)
    }
}

/// IP filtering result
#[derive(Debug, Clone, PartialEq)]
pub enum IpFilterResult {
    /// IP is allowed
    Allowed,
    /// IP is blocked
    Blocked,
}

impl IpFilterResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, IpFilterResult::Allowed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_ip_filter_allow_all() {
        let filter = IpFilter::new(vec![], true).unwrap();
        
        let test_ips = [
            "127.0.0.1",
            "192.168.1.1",
            "10.0.0.1",
            "::1",
            "2001:db8::1",
        ];
        
        for ip_str in &test_ips {
            let ip = IpAddr::from_str(ip_str).unwrap();
            assert_eq!(filter.check_ip(ip), IpFilterResult::Allowed);
        }
    }
    
    #[test]
    fn test_ip_filter_specific_ips() {
        let allowed = vec![
            "127.0.0.1".to_string(),
            "192.168.1.100".to_string(),
            "::1".to_string(),
        ];
        let filter = IpFilter::new(allowed, false).unwrap();
        
        // Allowed IPs
        assert_eq!(filter.check_ip("127.0.0.1".parse().unwrap()), IpFilterResult::Allowed);
        assert_eq!(filter.check_ip("192.168.1.100".parse().unwrap()), IpFilterResult::Allowed);
        assert_eq!(filter.check_ip("::1".parse().unwrap()), IpFilterResult::Allowed);
        
        // Blocked IPs
        assert_eq!(filter.check_ip("192.168.1.1".parse().unwrap()), IpFilterResult::Blocked);
        assert_eq!(filter.check_ip("10.0.0.1".parse().unwrap()), IpFilterResult::Blocked);
    }
    
    #[test]
    fn test_ip_filter_cidr() {
        let allowed = vec![
            "192.168.1.0/24".to_string(),
            "10.0.0.0/8".to_string(),
        ];
        let filter = IpFilter::new(allowed, false).unwrap();
        
        // Should allow IPs in the networks
        assert_eq!(filter.check_ip("192.168.1.1".parse().unwrap()), IpFilterResult::Allowed);
        assert_eq!(filter.check_ip("192.168.1.254".parse().unwrap()), IpFilterResult::Allowed);
        assert_eq!(filter.check_ip("10.1.2.3".parse().unwrap()), IpFilterResult::Allowed);
        assert_eq!(filter.check_ip("10.255.255.255".parse().unwrap()), IpFilterResult::Allowed);
        
        // Should block IPs outside the networks
        assert_eq!(filter.check_ip("192.168.2.1".parse().unwrap()), IpFilterResult::Blocked);
        assert_eq!(filter.check_ip("172.16.1.1".parse().unwrap()), IpFilterResult::Blocked);
        assert_eq!(filter.check_ip("127.0.0.1".parse().unwrap()), IpFilterResult::Blocked);
    }
    
    #[test]
    fn test_ip_network_contains() {
        let network = IpNetwork::from_str("192.168.1.0/24").unwrap();
        
        assert!(network.contains("192.168.1.1".parse().unwrap()));
        assert!(network.contains("192.168.1.254".parse().unwrap()));
        assert!(!network.contains("192.168.2.1".parse().unwrap()));
        assert!(!network.contains("10.0.0.1".parse().unwrap()));
    }
    
    #[test]
    fn test_ipv6_network() {
        let network = IpNetwork::from_str("2001:db8::/32").unwrap();
        
        assert!(network.contains("2001:db8::1".parse().unwrap()));
        assert!(network.contains("2001:db8:1::1".parse().unwrap()));
        assert!(!network.contains("2001:db9::1".parse().unwrap()));
        assert!(!network.contains("::1".parse().unwrap()));
    }
}