//! IAB TCF-based consent management.
//!
//! This module provides CMP-agnostic functionality for parsing and validating consent using
//! IAB Transparency and Consent Framework (TCF) v2. It works with any CMP that generates
//! standard euconsent-v2 cookies (Didomi, OneTrust, Cookiebot, etc.).
//!
//! The module handles:
//! - Parsing euconsent-v2 cookies containing TCF strings
//! - Checking vendor and purpose consent combinations
//! - Caching and validating against IAB Global Vendor List
//! - Providing flexible consent checking for any vendor/purpose combination

use fastly::Request;
use lib_tcstring::TcModelV2;
use log;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryFrom;

use crate::cookies;

/// IAB TCF Purpose IDs for common consent categories
pub mod purpose_ids {
    /// Purpose 1: Store and/or access information on a device
    pub const DEVICE_ACCESS: &[u8] = &[1];
    
    /// Advertising purposes: Basic ads + personalized ads
    /// - Purpose 2: Select basic ads
    /// - Purpose 3: Create a personalised ads profile
    /// - Purpose 4: Select personalised ads
    pub const ADVERTISING: &[u8] = &[2, 3, 4];
    
    /// Analytics purposes: Measurement and insights
    /// - Purpose 7: Measure ad performance
    /// - Purpose 8: Measure content performance
    /// - Purpose 9: Apply market research to generate audience insights
    pub const ANALYTICS: &[u8] = &[7, 8, 9];
    
    /// Basic advertising (non-personalized)
    /// - Purpose 2: Select basic ads only
    pub const BASIC_ADS: &[u8] = &[2];
}

/// IAB Global Vendor List entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorInfo {
    pub id: u16,
    pub name: String,
    pub purposes: Vec<u8>,
    pub legitimate_interests: Vec<u8>,
    pub features: Vec<u8>,
    pub special_features: Vec<u8>,
}

/// IAB Global Vendor List cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorList {
    pub vendors: HashMap<u16, VendorInfo>,
    pub last_updated: i64,
    pub version: u32,
}

impl VendorList {
    /// Creates an empty vendor list
    pub fn new() -> Self {
        Self {
            vendors: HashMap::new(),
            last_updated: chrono::Utc::now().timestamp(),
            version: 0,
        }
    }
    
    /// Checks if a vendor ID exists in the Global Vendor List
    pub fn is_valid_vendor(&self, vendor_id: u16) -> bool {
        self.vendors.contains_key(&vendor_id)
    }
    
    /// Gets vendor information by ID
    pub fn get_vendor(&self, vendor_id: u16) -> Option<&VendorInfo> {
        self.vendors.get(&vendor_id)
    }
    
    /// Checks if vendor declares a specific purpose
    pub fn vendor_declares_purpose(&self, vendor_id: u16, purpose_id: u8) -> bool {
        if let Some(vendor) = self.get_vendor(vendor_id) {
            vendor.purposes.contains(&purpose_id) || vendor.legitimate_interests.contains(&purpose_id)
        } else {
            false
        }
    }
}

/// TCF-based consent information from any CMP.
///
/// CMP-agnostic structure that works with any IAB TCF v2 compliant CMP.
/// Provides vendor + purpose consent checking as required by TCF specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcfConsent {
    /// Original TCF consent string from euconsent-v2 cookie
    pub tc_string: String,
    
    /// Whether GDPR regulations apply to this user
    pub gdpr_applies: bool,
    
    /// Purpose consent map: Purpose ID → user consent
    pub purpose_consents: HashMap<u8, bool>,
    
    /// Vendor consent map: Vendor ID → user consent  
    pub vendor_consents: HashMap<u16, bool>,
    
    /// Unix timestamp when consent was processed
    pub timestamp: i64,
    
    /// TCF version (should be "2" for TCF v2)
    pub version: String,
}

impl TcfConsent {
    /// Creates TcfConsent from a parsed TCF model.
    ///
    /// Extracts purpose and vendor consents from the TCF string data.
    pub fn from_tc_model(tc_model: TcModelV2, tc_string: String) -> Result<Self, String> {
        // Extract purpose consents from TcModelV2
        // From debug output: purposes_consent: [1, 2, 3]
        let mut purpose_consents = HashMap::new();
        for purpose_id in &tc_model.purposes_consent {
            purpose_consents.insert(*purpose_id, true);
        }
        
        // Extract vendor consents from TcModelV2  
        // From debug output: vendors_consent: [2, 6, 8]
        let mut vendor_consents = HashMap::new();
        for vendor_id in &tc_model.vendors_consent {
            vendor_consents.insert(*vendor_id, true);
        }
        
        // Determine if GDPR applies based on TCF data
        // For now, assume GDPR applies if we have a valid TCF string
        let gdpr_applies = !tc_string.is_empty();
        
        log::info!(
            "Parsed TCF consent: {} purposes, {} vendors, GDPR applies: {}",
            purpose_consents.len(),
            vendor_consents.len(),
            gdpr_applies
        );
        
        Ok(Self {
            tc_string,
            gdpr_applies,
            purpose_consents,
            vendor_consents,
            timestamp: chrono::Utc::now().timestamp(),
            version: "2".to_string(),
        })
    }
    
    /// Checks if a specific vendor has consent for given purposes.
    ///
    /// This is the core consent validation method implementing TCF v2 logic:
    /// - Vendor consent must be true
    /// - ALL specified purposes must have consent
    /// - If either fails, returns false
    ///
    /// # Arguments
    /// * `vendor_id` - IAB Global Vendor List ID
    /// * `purposes` - Array of purpose IDs to check
    /// * `vendor_list` - Optional vendor list for validation
    ///
    /// # Returns
    /// * `true` if vendor AND all purposes have consent
    /// * `false` if vendor or any purpose lacks consent
    pub fn has_consent(&self, vendor_id: u16, purposes: &[u8], vendor_list: Option<&VendorList>) -> bool {
        // Validate vendor exists in Global Vendor List if provided
        if let Some(vl) = vendor_list {
            if !vl.is_valid_vendor(vendor_id) {
                log::warn!("Vendor {} not found in Global Vendor List", vendor_id);
                return false;
            }
            
            // Check if vendor declares all required purposes
            for &purpose_id in purposes {
                if !vl.vendor_declares_purpose(vendor_id, purpose_id) {
                    log::warn!(
                        "Vendor {} does not declare purpose {} in Global Vendor List", 
                        vendor_id, 
                        purpose_id
                    );
                    return false;
                }
            }
        }
        
        // Check vendor consent in TCF string
        let vendor_consent = self.vendor_consents.get(&vendor_id).unwrap_or(&false);
        if !vendor_consent {
            log::debug!("Vendor {} consent denied in TCF string", vendor_id);
            return false;
        }
        
        // Check all purpose consents in TCF string
        for &purpose_id in purposes {
            let purpose_consent = self.purpose_consents.get(&purpose_id).unwrap_or(&false);
            if !purpose_consent {
                log::debug!("Purpose {} consent denied for vendor {} in TCF string", purpose_id, vendor_id);
                return false;
            }
        }
        
        log::debug!(
            "Consent granted for vendor {} with purposes {:?}", 
            vendor_id, 
            purposes
        );
        true
    }
    
    /// Convenience method: Checks basic advertising consent (Purpose 2 only)
    pub fn has_basic_advertising_consent(&self, vendor_id: u16, vendor_list: Option<&VendorList>) -> bool {
        self.has_consent(vendor_id, purpose_ids::BASIC_ADS, vendor_list)
    }
    
    /// Convenience method: Checks personalized advertising consent (Purposes 2, 3, 4)
    pub fn has_personalized_advertising_consent(&self, vendor_id: u16, vendor_list: Option<&VendorList>) -> bool {
        self.has_consent(vendor_id, purpose_ids::ADVERTISING, vendor_list)
    }
    
    /// Convenience method: Checks analytics consent (Purposes 7, 8, 9)
    pub fn has_analytics_consent(&self, vendor_id: u16, vendor_list: Option<&VendorList>) -> bool {
        self.has_consent(vendor_id, purpose_ids::ANALYTICS, vendor_list)
    }
    
    /// Convenience method: Checks functional consent (Purpose 1)
    pub fn has_functional_consent(&self, vendor_id: u16, vendor_list: Option<&VendorList>) -> bool {
        self.has_consent(vendor_id, purpose_ids::DEVICE_ACCESS, vendor_list)
    }
    
    /// Determines the appropriate consent level for advertising
    pub fn get_advertising_consent_level(&self, vendor_id: u16, vendor_list: Option<&VendorList>) -> AdvertisingConsentLevel {
        if self.has_personalized_advertising_consent(vendor_id, vendor_list) {
            AdvertisingConsentLevel::Personalized
        } else if self.has_basic_advertising_consent(vendor_id, vendor_list) {
            AdvertisingConsentLevel::BasicOnly
        } else {
            AdvertisingConsentLevel::None
        }
    }
}

/// Advertising consent levels for graduated consent handling
#[derive(Debug, Clone, PartialEq)]
pub enum AdvertisingConsentLevel {
    /// Full personalized advertising allowed
    Personalized,
    /// Only basic (non-personalized) advertising allowed
    BasicOnly,
    /// No advertising allowed
    None,
}

impl Default for TcfConsent {
    /// Creates default consent with all permissions denied.
    ///
    /// Used as fallback when no valid consent is found.
    /// Follows "privacy by default" principle - everything is false.
    fn default() -> Self {
        Self {
            tc_string: String::new(),
            gdpr_applies: false, // Default false as specified
            purpose_consents: HashMap::new(),
            vendor_consents: HashMap::new(),
            timestamp: chrono::Utc::now().timestamp(),
            version: "2".to_string(),
        }
    }
}

/// Extracts TCF consent from any CMP via euconsent-v2 cookie.
///
/// CMP-agnostic function that works with Didomi, OneTrust, Cookiebot, etc.
/// Looks for the standard euconsent-v2 cookie containing the IAB TCF consent string.
///
/// # Arguments
/// * `req` - HTTP request containing cookies
///
/// # Returns
/// * `Some(TcfConsent)` if valid TCF consent found
/// * `None` if no consent cookie or parsing fails (caller should use default)
pub fn get_tcf_consent_from_request(req: &Request) -> Option<TcfConsent> {
    match cookies::handle_request_cookies(req) {
        Ok(Some(jar)) => {
            // Look for euconsent-v2 cookie (standard IAB TCF cookie name)
            if let Some(euconsent_cookie) = jar.get("euconsent-v2") {
                let tc_string = euconsent_cookie.value();
                log::debug!("Found euconsent-v2 cookie: {}", tc_string);
                
                // Parse TCF string using lib_tcstring
                match TcModelV2::try_from(tc_string) {
                    Ok(tc_model) => {
                        log::info!("Successfully parsed TCF consent string");
                        match TcfConsent::from_tc_model(tc_model, tc_string.to_string()) {
                            Ok(consent) => return Some(consent),
                            Err(e) => log::warn!("Failed to create TcfConsent from TCF model: {}", e),
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to parse TCF consent string: {:?}", e);
                    }
                }
            } else {
                log::debug!("No euconsent-v2 cookie found");
            }
            None
        }
        Ok(None) => {
            log::debug!("No cookies found in request");
            None
        }
        Err(e) => {
            log::warn!("Failed to parse cookies for TCF consent: {:?}", e);
            None
        }
    }
}

/// TODO: Vendor list management functions
/// These would be implemented to fetch and cache the IAB Global Vendor List
pub mod vendor_list_manager {
    use super::*;
    
    /// Fetches the latest IAB Global Vendor List
    /// TODO: Implement HTTP fetch from https://vendor-list.consensu.org/v3/vendor-list.json
    pub async fn fetch_vendor_list() -> Result<VendorList, String> {
        // Implementation would:
        // 1. Fetch JSON from IAB endpoint
        // 2. Parse into VendorList structure
        // 3. Cache in KV store with TTL
        Err("Not implemented yet".to_string())
    }
    
    /// Gets cached vendor list or fetches if stale
    /// TODO: Implement KV store caching with weekly refresh
    pub async fn get_vendor_list() -> Result<VendorList, String> {
        // Implementation would:
        // 1. Check KV store for cached list
        // 2. If missing or older than 1 week, fetch new
        // 3. Return cached or fresh list
        Err("Not implemented yet".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fastly::Request;
    
    #[test]
    fn test_tcf_consent_default() {
        let consent = TcfConsent::default();
        assert_eq!(consent.tc_string, "");
        assert!(!consent.gdpr_applies);
        assert!(consent.purpose_consents.is_empty());
        assert!(consent.vendor_consents.is_empty());
        assert_eq!(consent.version, "2");
        assert!(consent.timestamp > 0);
    }
    
    #[test]
    fn test_vendor_list_validation() {
        let mut vendor_list = VendorList::new();
        
        // Add test vendor
        vendor_list.vendors.insert(45, VendorInfo {
            id: 45,
            name: "Equativ".to_string(),
            purposes: vec![1, 2, 3, 4, 7],
            legitimate_interests: vec![],
            features: vec![],
            special_features: vec![],
        });
        
        assert!(vendor_list.is_valid_vendor(45));
        assert!(!vendor_list.is_valid_vendor(999));
        assert!(vendor_list.vendor_declares_purpose(45, 2));
        assert!(!vendor_list.vendor_declares_purpose(45, 99));
    }
    
    #[test]
    fn test_advertising_consent_levels() {
        let mut consent = TcfConsent::default();
        let vendor_id = 45u16;
        
        // Test no consent
        assert_eq!(
            consent.get_advertising_consent_level(vendor_id, None),
            AdvertisingConsentLevel::None
        );
        
        // Grant vendor consent
        consent.vendor_consents.insert(vendor_id, true);
        
        // Test basic advertising only
        consent.purpose_consents.insert(2, true);
        assert_eq!(
            consent.get_advertising_consent_level(vendor_id, None),
            AdvertisingConsentLevel::BasicOnly
        );
        
        // Test personalized advertising
        consent.purpose_consents.insert(3, true);
        consent.purpose_consents.insert(4, true);
        assert_eq!(
            consent.get_advertising_consent_level(vendor_id, None),
            AdvertisingConsentLevel::Personalized
        );
    }
    
    #[test]
    fn test_get_tcf_consent_no_cookie() {
        let req = Request::get("https://example.com");
        let consent = get_tcf_consent_from_request(&req);
        assert!(consent.is_none());
    }
}