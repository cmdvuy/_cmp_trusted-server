# Didomi CMP Reverse Proxy Documentation

This document explains how to configure the Didomi CMP and Fastly Compute to create a reverse proxy that serves the Didomi Consent Management Platform (CMP) from your own domain.

## Overview

The Didomi CMP reverse proxy allows you to serve Didomi's SDK and API through your own domain instead of directly from Didomi's servers. This provides several benefits:

- **Domain Consistency**: All CMP resources are served from your domain
- **Enhanced Privacy**: First-party context for privacy compliance
- **Better Performance**: Edge caching and optimization through Fastly
- **Custom Control**: Ability to modify headers, add logging, or implement custom logic

## Architecture

```
User Request → Fastly Edge → Trusted Server (WASM) → Didomi Backends
                                    ↓
            didotest.com/consent/* → sdk.privacy-center.org
            didotest.com/consent/api/* → api.privacy-center.org
```

## Request Routing

The proxy implements intelligent routing based on the request path:

| Request Path | Backend | Destination | Purpose |
|-------------|---------|-------------|---------|
| `/consent/*` | `didomi_sdk` | `sdk.privacy-center.org` | Didomi SDK files (JS, CSS, etc.) |
| `/consent/api/*` | `didomi_api` | `api.privacy-center.org` | Didomi API endpoints |

## Implementation Details

### Core Components

1. **DidomiProxy Struct** (`src/didomi.rs`): Handles all reverse proxy logic
2. **Route Handler** (`src/main.rs`): Matches `/consent/*` paths and delegates to proxy
3. **Backend Configuration**: Fastly backends pointing to Didomi servers

### Key Features

- **Method Support**: GET, POST, PUT, DELETE, HEAD, OPTIONS
- **Header Forwarding**: Preserves essential headers (User-Agent, Authorization, etc.)
- **Geographic Information**: Forwards geo headers for location-based features
- **Query String Preservation**: Maintains all query parameters
- **Request Body Handling**: Supports POST/PUT request bodies
- **CORS Support**: Adds appropriate CORS headers for SDK requests

## Configuration Guide

### 1. Fastly Service Setup

#### Create Backends

Configure the following backends in your Fastly service:

```toml
# For local development in fastly.toml
[local_server.backends.didomi_sdk]
url = "https://sdk.privacy-center.org"

[local_server.backends.didomi_api]
url = "https://api.privacy-center.org"
```

For production, use the Fastly CLI:

```bash
# Create SDK backend
fastly backend create --service-id=YOUR_SERVICE_ID --version=VERSION \
  --name=didomi_sdk --address=sdk.privacy-center.org --port=443

# Create API backend  
fastly backend create --service-id=YOUR_SERVICE_ID --version=VERSION \
  --name=didomi_api --address=api.privacy-center.org --port=443
```

#### Backend Settings

Recommended backend configuration:
- **Port**: 443 (HTTPS)
- **Use SSL**: Yes
- **SSL SNI Hostname**: Match the backend address
- **Host Header**: Use backend address

### 2. DNS Configuration

Configure your domain to point to Fastly:

```
# Add CNAME record
didotest.com. IN CNAME global.fastly.com.
```

### 3. TLS Certificate

Ensure your domain has a valid TLS certificate configured in Fastly for HTTPS support.

### 4. Code Implementation

The proxy is implemented in the `DidomiProxy::handle_consent_request` method:

```rust
// Route matching in main.rs
(_, path) if path.starts_with("/consent/") => 
    DidomiProxy::handle_consent_request(&settings, req).await,

// Backend selection logic
let (backend_name, origin_path) = if consent_path.starts_with("/api/") {
    ("didomi_api", consent_path)    // API requests
} else {
    ("didomi_sdk", consent_path)    // SDK requests
};

// Request creation
let mut proxy_req = Request::new(req.get_method().clone(), full_url);
```

## Deployment

### Build and Deploy

```bash
# Build for production
cargo build --bin trusted-server-fastly --release --target wasm32-wasip1

# Deploy to Fastly
fastly compute publish
```

### Verification

Test the deployment:

```bash
# Test SDK route
curl https://yourdomain.com/consent/ORGANIZATION_ID/loader.js

# Test API route (should return 405 for GET)
curl https://yourdomain.com/consent/api/events

# Verify headers and response
curl -v https://yourdomain.com/consent/ORGANIZATION_ID/loader.js
```

## Usage Examples

### Basic SDK Integration

Replace the standard Didomi SDK integration:

```html
<!-- Before: Direct Didomi -->
<script src="https://sdk.privacy-center.org/YOUR_ORG_ID/loader.js"></script>

<!-- After: Through your proxy -->
<script src="https://yourdomain.com/consent/YOUR_ORG_ID/loader.js"></script>
```

### API Requests

Update API endpoints:

```javascript
// Before: Direct Didomi API
fetch('https://api.privacy-center.org/events', {
    method: 'POST',
    body: eventData
});

// After: Through your proxy
fetch('https://yourdomain.com/consent/api/events', {
    method: 'POST', 
    body: eventData
});
```

## Header Handling

### Forwarded Headers

The proxy forwards these essential headers:
- `Accept`, `Accept-Language`, `Accept-Encoding`
- `User-Agent`, `Referer`, `Origin`
- `Authorization`
- `Content-Type` (for POST/PUT)
- `X-Forwarded-For` (client IP)

### Geographic Headers

For SDK requests, geographic information is forwarded:
- `X-Geo-Country` ← `FastlyGeo-CountryCode`
- `X-Geo-Region` ← `FastlyGeo-Region`  
- `CloudFront-Viewer-Country` ← `FastlyGeo-CountryCode`

### Response Headers

The proxy adds CORS headers for SDK requests:
- `Access-Control-Allow-Origin: *`
- `Access-Control-Allow-Headers: Content-Type, Authorization, X-Requested-With`
- `Access-Control-Allow-Methods: GET, POST, PUT, DELETE, OPTIONS`

## Troubleshooting

### Common Issues

1. **500 Internal Server Error**
   - Check backend configuration
   - Verify backend names match code expectations
   - Ensure backends are accessible from Fastly

2. **404 Not Found**
   - Verify route matching in main.rs
   - Check path prefix `/consent/`

3. **CORS Errors**
   - Ensure CORS headers are properly set
   - Check origin restrictions

4. **Backend Connection Issues**
   - Verify backend URLs and ports
   - Check SSL configuration
   - Test backend accessibility

### Debugging

Enable verbose logging and check Fastly logs:

```bash
# View real-time logs
fastly log-tail --service-id=YOUR_SERVICE_ID

# Check backend status
fastly backend list --service-id=YOUR_SERVICE_ID --version=active
```

### Testing Individual Components

```bash
# Test backend connectivity
curl -H "Host: sdk.privacy-center.org" https://your-fastly-service.com/test

# Test without proxy
curl https://sdk.privacy-center.org/YOUR_ORG_ID/loader.js

# Compare responses
diff <(curl -s https://sdk.privacy-center.org/path) \
     <(curl -s https://yourdomain.com/consent/path)
```

## Security Considerations

### Header Security

- The proxy does not forward cookies by default (as per Didomi documentation)
- Sensitive headers are filtered appropriately
- Client IP is preserved in `X-Forwarded-For`

### Backend Security

- All backend communication uses HTTPS
- SNI is properly configured for SSL verification
- Backend hosts are validated

### Origin Validation

Consider implementing origin validation for production:

```rust
// Example: Restrict origins
if let Some(origin) = req.get_header(header::ORIGIN) {
    if !allowed_origins.contains(&origin.to_str().unwrap_or("")) {
        return Ok(Response::from_status(StatusCode::FORBIDDEN));
    }
}
```

## Performance Optimization

### Caching Strategy

The proxy preserves cache headers from Didomi backends:
- SDK files: Long-term caching (handled by Didomi)
- API responses: No caching (dynamic content)

### Edge Optimization

- Requests are processed at Fastly edge locations
- Geographic routing optimizes backend selection
- Compression is enabled via `x-compress-hint` header

## Monitoring

### Key Metrics

Monitor these metrics in Fastly:
- Request volume to `/consent/*` paths
- Backend response times
- Error rates (4xx/5xx responses)
- Cache hit rates for SDK content

### Alerting

Set up alerts for:
- High error rates
- Backend connectivity issues  
- Unusual traffic patterns
- Performance degradation

## Compliance Notes

### Privacy Compliance

The reverse proxy maintains compliance by:
- Preserving all Didomi privacy mechanisms
- Forwarding geographic information for regulation compliance
- Maintaining user consent state through proper header handling

### Data Processing

- No user data is stored or logged by the proxy
- All requests are passed through transparently
- Geographic information is only used for routing optimization

## Support

For issues specific to:
- **Didomi CMP**: Contact Didomi support
- **Fastly Platform**: Contact Fastly support  
- **Proxy Implementation**: Check this documentation and code comments

## Changelog

### Version 1.0
- Initial implementation with SDK and API routing
- CORS support for cross-origin requests
- Geographic header forwarding
- Comprehensive error handling