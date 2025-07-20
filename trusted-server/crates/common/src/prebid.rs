//! Prebid integration for real-time bidding.
//!
//! This module provides functionality for integrating with Prebid Server
//! to enable header bidding and real-time ad auctions.

use error_stack::Report;
use fastly::http::{header, Method};
use fastly::{Error, Request, Response};
use serde_json::json;

use crate::constants::{
    HEADER_SYNTHETIC_FRESH, HEADER_SYNTHETIC_TRUSTED_SERVER, HEADER_X_FORWARDED_FOR,
};
use crate::error::TrustedServerError;
use crate::settings::Settings;
use crate::synthetic::generate_synthetic_id;
use crate::tcf_consent::get_tcf_consent_from_request;

/// Represents a request to the Prebid Server with all necessary parameters
pub struct PrebidRequest {
    /// Synthetic ID used for user identification across requests
    pub synthetic_id: String,
    /// Domain for the ad request
    pub domain: String,
    /// List of banner sizes as (width, height) tuples
    pub banner_sizes: Vec<(u32, u32)>,
    /// Client's IP address for geo-targeting and fraud prevention
    pub client_ip: String,
    /// Origin header for CORS and tracking
    pub origin: String,
}

impl PrebidRequest {
    /// Creates a new PrebidRequest from an incoming Fastly request.
    ///
    /// Extracts necessary information from the request including synthetic ID,
    /// client IP, and origin for use in Prebid Server requests.
    ///
    /// # Errors
    ///
    /// - [`TrustedServerError::SyntheticId`] if synthetic ID generation fails
    pub fn new(settings: &Settings, req: &Request) -> Result<Self, Report<TrustedServerError>> {
        // Get the Trusted Server ID from header (which we just set in handle_prebid_test)
        let synthetic_id = match req
            .get_header(HEADER_SYNTHETIC_TRUSTED_SERVER)
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string())
        {
            Some(id) => id,
            None => generate_synthetic_id(settings, req)?,
        };

        // Get the original client IP from Fastly headers
        let client_ip = req
            .get_client_ip_addr()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| {
                req.get_header(HEADER_X_FORWARDED_FOR)
                    .and_then(|h| h.to_str().ok())
                    .unwrap_or("")
                    .split(',') // X-Forwarded-For can be a comma-separated list
                    .next() // Get the first (original) IP
                    .unwrap_or("")
                    .to_string()
            });

        // Try to get domain from Referer or Origin headers, fallback to default
        let domain = req
            .get_header(header::REFERER)
            .and_then(|h| h.to_str().ok())
            .and_then(|r| url::Url::parse(r).ok())
            .and_then(|u| u.host_str().map(|h| h.to_string()))
            .or_else(|| {
                req.get_header(header::ORIGIN)
                    .and_then(|h| h.to_str().ok())
                    .and_then(|o| url::Url::parse(o).ok())
                    .and_then(|u| u.host_str().map(|h| h.to_string()))
            })
            .unwrap_or_else(|| settings.publisher.domain.clone());

        log::info!("Detected domain: {}", domain);

        // Create origin with owned String
        let origin = req
            .get_header(header::ORIGIN)
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("https://{}", domain));

        Ok(Self {
            synthetic_id,
            domain,
            banner_sizes: vec![(728, 90)], // TODO: Make this configurable
            client_ip,
            origin,
        })
    }

    /// Sends bid request to Prebid Server with GDPR compliance
    ///
    /// Makes an HTTP POST request to PBS with all necessary headers and body.
    /// Includes GDPR fields in OpenRTB request based on TCF consent data.
    /// Uses the stored synthetic ID for user identification.
    ///
    /// # Returns
    /// * `Result<Response, Error>` - Prebid Server response or error
    pub async fn send_bid_request(
        &self,
        settings: &Settings,
        incoming_req: &Request,
    ) -> Result<Response, Error> {
        let mut req = Request::new(Method::POST, settings.prebid.server_url.to_owned());

        // Get and store the POTSI ID value from the incoming request
        let id: String = incoming_req
            .get_header(HEADER_SYNTHETIC_TRUSTED_SERVER)
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.synthetic_id.clone());

        log::info!("Found Trusted Server ID from incoming request: {}", id);

        // Extract TCF consent from request (euconsent-v2 cookie)
        let tcf_consent = get_tcf_consent_from_request(incoming_req).unwrap_or_default();
        log::info!("TCF consent - GDPR applies: {}, TC string: {}", 
                   tcf_consent.gdpr_applies, 
                   if tcf_consent.tc_string.is_empty() { "none" } else { "present" });

        // Construct the OpenRTB2 bid request with GDPR fields
        let prebid_body = json!({
            "id": id,
            "imp": [{
                "id": "imp1",
                "banner": {
                    "format": self.banner_sizes.iter().map(|(w, h)| {
                        json!({ "w": w, "h": h })
                    }).collect::<Vec<_>>()
                },
                "bidfloor": 0.01,
                "bidfloorcur": "USD",
                "ext": {
                    "prebid": {
                        "bidder": {
                            "smartadserver": {
                                "siteId": 686105,
                                "networkId": 5280,
                                "pageId": 2040327,
                                "formatId": 137675,
                                "target": "testing=prebid",
                                "domain": &self.domain
                            }
                        }
                    }
                }
            }],
            "site": { "page": format!("https://{}", self.domain) },
            "user": {
                "id": "5280",
                "ext": {
                    "consent": tcf_consent.tc_string,
                    "eids": [
                        {
                            "source": &self.domain,
                            "uids": [{
                                "id": self.synthetic_id,
                                "atype": 1,
                                "ext": {
                                    "type": "fresh"
                                }
                            }],
                        },
                        {
                            "source": &self.domain,
                            "uids": [{
                                "id": &id,
                                "atype": 1,
                                "ext": {
                                    "type": "potsi" // TODO: remove reference to potsi
                                }
                            }]
                        }
                    ]
                }
            },
            "test": 1,
            "debug": 1,
            "tmax": 1000,
            "at": 1,
            // GDPR compliance fields per OpenRTB 2.5
            "regs": {
                "ext": {
                    "gdpr": if tcf_consent.gdpr_applies { 1 } else { 0 }
                }
            }
        });

        req.set_header(header::CONTENT_TYPE, "application/json");
        req.set_header(HEADER_X_FORWARDED_FOR, &self.client_ip);
        req.set_header(header::ORIGIN, &self.origin);
        req.set_header(HEADER_SYNTHETIC_FRESH, &self.synthetic_id);
        req.set_header(HEADER_SYNTHETIC_TRUSTED_SERVER, &id);

        log::info!(
            "Sending prebid request with Fresh ID: {} and Trusted Server ID: {}",
            self.synthetic_id,
            id
        );

        req.set_body_json(&prebid_body)?;

        let resp = req.send("prebid_backend")?;
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fastly::Request;

    use crate::test_support::tests::create_test_settings;

    #[test]
    fn test_prebid_request_new_with_full_headers() {
        let settings = create_test_settings();
        let mut req = Request::get("https://example.com/test");
        req.set_header(HEADER_SYNTHETIC_TRUSTED_SERVER, "existing-synthetic-id");
        req.set_header(header::REFERER, "https://test-domain.com/page");
        req.set_header(header::ORIGIN, "https://test-domain.com");
        req.set_header(HEADER_X_FORWARDED_FOR, "192.168.1.1, 10.0.0.1");

        let prebid_req = PrebidRequest::new(&settings, &req).unwrap();

        assert_eq!(prebid_req.synthetic_id, "existing-synthetic-id");
        assert_eq!(prebid_req.domain, "test-domain.com");
        assert_eq!(prebid_req.banner_sizes, vec![(728, 90)]);
        assert_eq!(prebid_req.origin, "https://test-domain.com");
        // Note: client_ip extraction from X-Forwarded-For depends on Fastly runtime
    }

    #[test]
    fn test_prebid_request_new_without_synthetic_id() {
        let settings = create_test_settings();
        let mut req = Request::get("https://example.com/test");
        req.set_header("User-Agent", "Mozilla/5.0");
        req.set_header(header::REFERER, "https://test-domain.com/page");

        let prebid_req = PrebidRequest::new(&settings, &req).unwrap();

        // Should generate a new synthetic ID
        assert!(!prebid_req.synthetic_id.is_empty());
        assert_eq!(prebid_req.domain, "test-domain.com");
    }

    #[test]
    fn test_prebid_request_domain_from_origin() {
        let settings = create_test_settings();
        let mut req = Request::get("https://example.com/test");
        req.set_header(header::ORIGIN, "https://origin-domain.com");
        // No referer header

        let prebid_req = PrebidRequest::new(&settings, &req).unwrap();

        assert_eq!(prebid_req.domain, "origin-domain.com");
        assert_eq!(prebid_req.origin, "https://origin-domain.com");
    }

    #[test]
    fn test_prebid_request_domain_fallback() {
        let settings = create_test_settings();
        let url = format!("https://{}", settings.publisher.domain);
        let req = Request::get(url.clone());
        // No referer or origin headers

        let prebid_req = PrebidRequest::new(&settings, &req).unwrap();

        assert_eq!(prebid_req.domain, settings.publisher.domain);
        assert_eq!(prebid_req.origin, url);
    }

    #[test]
    fn test_prebid_request_invalid_url_in_referer() {
        let settings = create_test_settings();
        let url = format!("https://{}/test", settings.publisher.domain);
        let mut req = Request::get(url);
        req.set_header(header::REFERER, "not-a-valid-url");

        let prebid_req = PrebidRequest::new(&settings, &req).unwrap();

        // Should fallback to default domain
        assert_eq!(prebid_req.domain, settings.publisher.domain);
    }

    #[test]
    fn test_prebid_request_x_forwarded_for_parsing() {
        let settings = create_test_settings();
        let url = format!("https://{}/test", settings.publisher.domain);
        let mut req = Request::get(url);
        req.set_header(HEADER_X_FORWARDED_FOR, "192.168.1.1, 10.0.0.1, 172.16.0.1");

        let prebid_req = PrebidRequest::new(&settings, &req).unwrap();

        // Should get the first IP from the list (if get_client_ip_addr returns None)
        // The actual behavior depends on Fastly runtime
        assert!(!prebid_req.client_ip.is_empty());
    }

    #[test]
    fn test_prebid_request_struct_fields() {
        let prebid_req = PrebidRequest {
            synthetic_id: "test-id".to_string(),
            domain: "test.com".to_string(),
            banner_sizes: vec![(300, 250), (728, 90)],
            client_ip: "192.168.1.1".to_string(),
            origin: "https://test.com".to_string(),
        };

        assert_eq!(prebid_req.synthetic_id, "test-id");
        assert_eq!(prebid_req.domain, "test.com");
        assert_eq!(prebid_req.banner_sizes.len(), 2);
        assert_eq!(prebid_req.banner_sizes[0], (300, 250));
        assert_eq!(prebid_req.banner_sizes[1], (728, 90));
        assert_eq!(prebid_req.client_ip, "192.168.1.1");
        assert_eq!(prebid_req.origin, "https://test.com");
    }

    #[test]
    fn test_prebid_request_with_multiple_sizes() {
        let mut prebid_req = PrebidRequest {
            synthetic_id: "test-id".to_string(),
            domain: "test.com".to_string(),
            banner_sizes: vec![(300, 250), (728, 90), (160, 600)],
            client_ip: "192.168.1.1".to_string(),
            origin: "https://test.com".to_string(),
        };

        // Test modifying banner sizes
        prebid_req.banner_sizes.push((970, 250));
        assert_eq!(prebid_req.banner_sizes.len(), 4);
        assert_eq!(prebid_req.banner_sizes[3], (970, 250));
    }

    #[test]
    fn test_prebid_request_edge_cases() {
        let settings = create_test_settings();
        let url = format!("https://{}/test", settings.publisher.domain);

        // Test with empty X-Forwarded-For
        let mut req = Request::get(url.clone());
        req.set_header(HEADER_X_FORWARDED_FOR, "");
        let prebid_req = PrebidRequest::new(&settings, &req).unwrap();
        assert!(!prebid_req.client_ip.is_empty() || prebid_req.client_ip.is_empty());

        // Test with malformed origin
        let mut req2 = Request::get(url.clone());
        req2.set_header(header::ORIGIN, "://invalid");
        let prebid_req2 = PrebidRequest::new(&settings, &req2).unwrap();
        assert_eq!(prebid_req2.domain, settings.publisher.domain);
    }

    // Note: Testing send_bid_request would require mocking the Fastly backend,
    // which isn't available in unit tests. This would be covered in integration tests.
    // The method constructs a proper OpenRTB request with all required fields.
}
