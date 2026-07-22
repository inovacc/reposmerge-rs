//! reposmerge application config — faithful minimal shim of Go's
//! `cmd/reposmerge/internal/app` + the CONFIG DATA of `mantle/bootstrap.Base`.
//!
//! ## Mantle boundary (faithful-scope decision, NOT a defect)
//! Go's `app.App` squash-embeds `mantle/bootstrap.Base`, and `main.go` calls
//! `bootstrap.Configure(root, app.New(), ...)`, which wires an entire framework
//! runtime: viper config-file loading, an OpenTelemetry observability pipeline,
//! a structured/redacting logger, and an optional daemon supervisor, all built
//! in cobra's `PersistentPreRunE`.
//!
//! reposmerge's OWN commands NEVER read that runtime — it is inert framework
//! plumbing. Per the porting rule "map an external framework, don't reimplement
//! it", the mantle runtime (viper / otel / logger / daemon) is **out of scope**
//! and deliberately NOT ported. This module reproduces only the inert CONFIG
//! DATA of `DefaultBase()` so the shape survives; nothing in the CLI reads it.
//!
//! The observable CLI surface (a global `--config/-c` flag + `--version`) is
//! reproduced in `main.rs`, not here.

/// Logger configuration (subset of mantle's logger config that `DefaultBase`
/// seeds). Inert: never read by any reposmerge command.
#[derive(Debug, Clone, PartialEq)]
pub struct LoggerConfig {
    pub level: String,
    pub format: String,
    pub redact: bool,
}

/// Observability configuration (subset of mantle's otel config seeded by
/// `DefaultBase`). Inert: never read by any reposmerge command.
#[derive(Debug, Clone, PartialEq)]
pub struct ObservabilityConfig {
    pub protocol: String,
    pub sample: f64,
    pub interval_secs: u64,
    pub runtime_metrics: bool,
}

/// Feature toggles mirrored from mantle's `Base.Features`.
#[derive(Debug, Clone, PartialEq)]
pub struct Features {
    pub logging: bool,
    pub observability: bool,
    pub daemon: bool,
}

/// Faithful minimal shim of `mantle/bootstrap.Base` — CONFIG DATA only.
/// Inert: reposmerge commands never read it (see module docs).
#[derive(Debug, Clone, PartialEq)]
pub struct Base {
    pub environment: String,
    pub features: Features,
    pub logger: LoggerConfig,
    pub observability: ObservabilityConfig,
}

/// Reproduces `mantle/bootstrap.DefaultBase()`'s seed values.
///
/// The exact mantle defaults live in an external module (out of scope); these
/// mirror the documented seed: environment "dev"; logging on, observability and
/// daemon off; logger level "info"/format "json"/redact on; observability gRPC
/// exporter, full sampling, 15s interval, runtime metrics on. Simplified to the
/// fields that matter for parity of the inert config shape.
pub fn default_base() -> Base {
    Base {
        environment: "dev".to_string(),
        features: Features {
            logging: true,
            observability: false,
            daemon: false,
        },
        logger: LoggerConfig {
            level: "info".to_string(),
            format: "json".to_string(),
            redact: true,
        },
        observability: ObservabilityConfig {
            protocol: "grpc".to_string(),
            sample: 1.0,
            interval_secs: 15,
            runtime_metrics: true,
        },
    }
}

/// reposmerge application config — Go `app.App` embedding `bootstrap.Base`.
/// Inert config holder; the CLI does not read it (mantle runtime is out of scope).
#[derive(Debug, Clone, PartialEq)]
pub struct App {
    pub base: Base,
}

/// Returns `App` seeded with defaults — Go `app.New()`.
pub fn new() -> App {
    App {
        base: default_base(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_seeds_default_base() {
        let a = new();
        assert_eq!(a.base.environment, "dev");
        assert!(a.base.features.logging);
        assert!(!a.base.features.daemon);
        assert_eq!(a.base.logger.level, "info");
        assert_eq!(a.base.logger.format, "json");
        assert!(a.base.logger.redact);
        assert_eq!(a.base.observability.protocol, "grpc");
        assert_eq!(a.base.observability.sample, 1.0);
        assert_eq!(a.base.observability.interval_secs, 15);
        assert!(a.base.observability.runtime_metrics);
    }
}
