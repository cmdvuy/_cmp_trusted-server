# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is Prebid Server - an open-source solution for running real-time advertising auctions in the cloud. It's part of the Prebid ecosystem and integrates with Prebid.js and Prebid Mobile SDKs to deliver header bidding for any ad format and digital media type.

## Repository Structure

### Core Directories

#### `/adapters/`
- **Purpose**: Bid adapter implementations for different SSPs/DSPs
- **Tech Stack**: Go, OpenRTB protocol
- **Key Features**: Transform OpenRTB requests/responses for bidding servers
- **Notable**: 100+ adapters (33across, appnexus, criteo, pubmatic, etc.)

#### `/config/`
- **Purpose**: Configuration management and validation
- **Key Files**: 
  - `config.go`: Main configuration structure
  - `bidderinfo.go`: Bidder-specific configurations
  - `stored_requests.go`: Stored request configurations

#### `/exchange/`
- **Purpose**: Core auction logic and bidder orchestration
- **Key Files**:
  - `exchange.go`: Main auction engine
  - `auction.go`: Auction processing logic
  - `bidder.go`: Bidder interface and management

#### `/endpoints/`
- **Purpose**: HTTP endpoint handlers
- **Key Endpoints**: `/openrtb2/auction`, `/cookie_sync`, `/status`

#### `/openrtb_ext/`
- **Purpose**: OpenRTB protocol extensions for Prebid
- **Features**: Custom bid parameters, floor prices, targeting

#### `/privacy/`
- **Purpose**: GDPR, CCPA, and privacy compliance
- **Key Files**:
  - `activitycontrol.go`: Privacy activity enforcement
  - `scrubber.go`: PII data scrubbing

### Supporting Directories

- `/analytics/`: Analytics modules for collecting auction data
- `/currency/`: Multi-currency support and conversion
- `/floors/`: Price floor enforcement
- `/gdpr/`: GDPR compliance and consent management
- `/metrics/`: Prometheus metrics and monitoring
- `/modules/`: Extensible auction modules system
- `/stored_requests/`: Configuration and request caching
- `/usersync/`: Cookie synchronization with bidders

## Technology Stack

### Core Technologies
- **Language**: Go 1.23+
- **Framework**: Custom HTTP server with `httprouter`
- **Protocol**: OpenRTB 2.6
- **Dependencies**: Managed via Go modules

### Key Libraries
- `github.com/prebid/openrtb/v20`: OpenRTB protocol implementation
- `github.com/julienschmidt/httprouter`: HTTP routing
- `github.com/spf13/viper`: Configuration management
- `github.com/prometheus/client_golang`: Metrics collection
- `github.com/prebid/go-gdpr`: GDPR compliance
- `github.com/coocood/freecache`: In-memory caching

### Build System
- **Build Tool**: Go modules with Makefile
- **Docker**: Multi-stage builds with Ubuntu 22.04
- **Dependencies**: Requires `gcc` for CGO modules and `libatomic1` at runtime

## Common Commands

### Development
```bash
# Setup dependencies
go mod download
go mod tidy

# Run development server
go run .                    # Default port 8000

# Build binary
go build .
make build                  # Build with full validation
```

### Testing
```bash
# Run all tests
./validate.sh              # Complete validation suite
go test ./...               # Unit tests only

# Testing with options
./validate.sh --race 10     # Race condition testing
./validate.sh --cov        # Coverage analysis
./validate.sh --nofmt      # Skip formatting

# Test specific adapter
make test adapter=appnexus
```

### Code Quality
```bash
# Format code
./scripts/format.sh -f true
make format

# Format check (no changes)
./scripts/format.sh -f false
make formatcheck

# Vet code
go vet ./...
```

### Docker
```bash
# Build image
docker build -t prebid-server .
make image

# Run container
docker run -p 8000:8000 prebid-server
```

### Coverage
```bash
# Check coverage (minimum 30%)
./scripts/check_coverage.sh
./scripts/coverage.sh --html  # Detailed HTML report
```

## Configuration

### Required Configuration
- **GDPR Default**: Must set default GDPR value (`"0"` or `"1"`)
- **External URL**: Required for proper operation
- **Bidder Configs**: Located in `/static/bidder-info/` and `/static/bidder-params/`

### Configuration Methods
1. **Configuration File**: JSON/YAML at `/etc/config` (Docker)
2. **Environment Variables**: Prefixed with `PBS_`
3. **Command Line Flags**: Via `glog` and custom flags

### Key Configuration Sections
- **Auction Timeouts**: Bidder-specific timeout settings
- **Privacy Settings**: GDPR, CCPA compliance
- **Analytics**: Event collection and forwarding
- **Stored Requests**: Caching and retrieval settings
- **Metrics**: Prometheus endpoint configuration

## Development Patterns

### Bid Adapter Development
1. **Location**: `/adapters/{bidder}/`
2. **Required Files**:
   - `{bidder}.go`: Main adapter implementation
   - `{bidder}_test.go`: Unit tests
   - `params_test.go`: Parameter validation tests
3. **Bidder Info**: JSON config in `/static/bidder-info/{bidder}.json`
4. **Parameter Schema**: JSON schema in `/static/bidder-params/{bidder}.json`

### Testing Strategy
- **Unit Tests**: Standard Go testing with `testify`
- **Integration Tests**: Adapter test framework in `/adapters/adapterstest/`
- **JSON Test Data**: Test cases in `/adapters/{bidder}/test/`
- **Coverage Requirement**: Minimum 30% code coverage
- **Race Testing**: Special `TestRace*` functions for concurrency

### Module Development
- **Location**: `/modules/`
- **Hook System**: Extensible auction pipeline hooks
- **Module Registration**: Auto-generated builder in `modules/builder.go`

## Architecture Overview

### Request Flow
1. **HTTP Request**: Received at endpoint (`/openrtb2/auction`)
2. **Request Validation**: OpenRTB validation and privacy checks
3. **Stored Requests**: Merge with cached configurations
4. **Auction**: Parallel bidder requests via exchange
5. **Response**: Aggregate bids and return OpenRTB response

### Privacy-First Design
- **GDPR Compliance**: Full TCF 2.0 support with vendor lists
- **CCPA Support**: California privacy law compliance
- **Activity Controls**: Granular privacy activity enforcement
- **Data Scrubbing**: Automatic PII removal when required

### Extension Points
- **Bid Adapters**: Custom bidder integrations
- **Analytics Modules**: Custom data collection
- **Auction Modules**: Pipeline modifications via hooks
- **Stored Requests**: Custom configuration sources

## Important Files

### Entry Points
- `main.go`: Application entry point and server initialization
- `router/router.go`: HTTP route definitions
- `server/server.go`: HTTP server setup

### Core Logic
- `exchange/exchange.go`: Main auction orchestration
- `exchange/auction.go`: Auction processing logic
- `adapters/bidder.go`: Bidder interface definition

### Configuration
- `config/config.go`: Main configuration structure
- `static/bidder-info/`: Bidder metadata and capabilities
- `static/bidder-params/`: JSON schemas for bidder parameters

## Testing Notes

### Running Tests
- Use `./validate.sh` for complete validation including formatting and vetting
- Individual adapter testing: `make test adapter={name}`
- Race condition testing with `--race` flag (tests named `TestRace*`)

### Test Organization
- Adapter tests in `/adapters/{bidder}/test/` with JSON test cases
- Unit tests alongside source files (`*_test.go`)
- Integration test utilities in `/adapters/adapterstest/`

### Coverage Requirements
- Minimum 30% code coverage enforced
- Use `./scripts/coverage.sh --html` for detailed coverage reports
- Coverage checking via `./scripts/check_coverage.sh`

## Deployment

### Docker Deployment
- **Official Image**: `prebid/prebid-server` on Docker Hub
- **Static Files**: Must include `/static` directory
- **Ports**: 8000 (main), 6060 (admin/debug)
- **User**: Runs as non-root user `prebid:prebidgroup`

### Binary Deployment
- **Build**: `go build .` produces standalone binary
- **Requirements**: `libatomic1` runtime dependency
- **Static Files**: Deploy `/static` directory alongside binary

### Configuration
- **Environment**: Use environment variables or config files
- **Kubernetes**: ConfigMaps for configuration management
- **Required**: Set GDPR default value before deployment

## Important Notes

- **GDPR Requirement**: Must configure default GDPR value before hosting
- **CGO Required**: Some modules require native C code compilation
- **Static Files**: Critical for startup - must be deployed with binary
- **Go Version**: Requires Go 1.23 or newer
- **OpenRTB Compliance**: Full OpenRTB 2.6 protocol support
- **Prebid Registration**: Consider registering as official Prebid Server host
- **No Import Support**: Not intended for use as Go module dependency
- **Multi-Currency**: Built-in currency conversion support
- **Floor Prices**: Advanced price floor enforcement capabilities