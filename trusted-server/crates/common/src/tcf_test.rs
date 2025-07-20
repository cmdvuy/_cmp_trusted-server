//! Test module to explore lib_tcstring TcModelV2 structure
//!
//! This module is used to understand the API of lib_tcstring and inspect
//! the fields available in TcModelV2 for extracting consent data.

use lib_tcstring::TcModelV2;
use std::convert::TryFrom;

/// Test function to inspect TcModelV2 structure with a sample TCF string
pub fn inspect_tcf_model() {
    // Sample TCF string (this is a real example from IAB documentation)
    let tcf_string = "COvFyGBOvFyGBAbAAAENAPCAAOAAAAAAAAAAAEEUACCKAAA";
    
    match TcModelV2::try_from(tcf_string) {
        Ok(tc_model) => {
            println!("Successfully parsed TCF string: {}", tcf_string);
            println!("TcModelV2 debug output: {:?}", tc_model);
            
            // Try to access common fields (these might fail if the API is different)
            // We'll see what fields are available by testing the debug output
            
        }
        Err(e) => {
            println!("Failed to parse TCF string: {:?}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tcf_parsing() {
        inspect_tcf_model();
    }
    
    #[test]
    fn test_multiple_tcf_strings() {
        // Test with different TCF strings to understand the structure
        let test_strings = vec![
            "COvFyGBOvFyGBAbAAAENAPCAAOAAAAAAAAAAAEEUACCKAAA",
            // Second string has invalid length, so we'll only test the first one
        ];
        
        for tcf_string in test_strings {
            println!("\n--- Testing TCF String: {} ---", tcf_string);
            match TcModelV2::try_from(tcf_string) {
                Ok(tc_model) => {
                    println!("✓ Parsed successfully");
                    println!("Debug: {:?}", tc_model);
                    
                    // Test our TcfConsent creation
                    use crate::tcf_consent::TcfConsent;
                    match TcfConsent::from_tc_model(tc_model, tcf_string.to_string()) {
                        Ok(consent) => {
                            println!("✓ Created TcfConsent successfully");
                            println!("  Purposes with consent: {:?}", consent.purpose_consents.keys().collect::<Vec<_>>());
                            println!("  Vendors with consent: {:?}", consent.vendor_consents.keys().collect::<Vec<_>>());
                            
                            // Test consent checking
                            let vendor_2 = 2u16;
                            let vendor_999 = 999u16;
                            
                            println!("  Basic advertising consent for vendor 2: {}", 
                                consent.has_basic_advertising_consent(vendor_2, None));
                            println!("  Personalized advertising consent for vendor 2: {}", 
                                consent.has_personalized_advertising_consent(vendor_2, None));
                            println!("  Analytics consent for vendor 2: {}", 
                                consent.has_analytics_consent(vendor_2, None));
                            println!("  Functional consent for vendor 2: {}", 
                                consent.has_functional_consent(vendor_2, None));
                                
                            println!("  Basic advertising consent for vendor 999 (not consented): {}", 
                                consent.has_basic_advertising_consent(vendor_999, None));
                        }
                        Err(e) => {
                            println!("✗ Failed to create TcfConsent: {}", e);
                        }
                    }
                }
                Err(e) => {
                    println!("✗ Failed to parse: {:?}", e);
                }
            }
        }
    }
}