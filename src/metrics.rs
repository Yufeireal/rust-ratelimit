use prometheus::{
    Counter, CounterVec, GaugeVec, Histogram, HistogramOpts, HistogramVec, Opts, Registry,
};
use std::sync::Arc;

/// Metrics collector for the rate limit service
#[derive(Clone)]
pub struct Metrics {
    registry: Arc<Registry>,
    
    // Rate limit metrics
    total_requests: CounterVec,
    over_limit_requests: CounterVec,
    near_limit_requests: CounterVec,
    within_limit_requests: CounterVec,
    shadow_mode_requests: CounterVec,
    
    // Cache metrics
    local_cache_hits: Counter,
    local_cache_misses: Counter,
    
    // Redis metrics
    redis_operations: CounterVec,
    redis_operation_duration: HistogramVec,
    redis_connection_active: GaugeVec,
    
    // Service metrics
    config_load_success: Counter,
    config_load_error: Counter,
    request_duration: Histogram,
}

impl Metrics {
    /// Create a new metrics instance
    pub fn new() -> prometheus::Result<Self> {
        let registry = Arc::new(Registry::new());

        let total_requests = CounterVec::new(
            Opts::new(
                "ratelimit_total_requests",
                "Total number of rate limit requests",
            ),
            &["domain", "descriptor"],
        )?;

        let over_limit_requests = CounterVec::new(
            Opts::new(
                "ratelimit_over_limit_requests",
                "Number of requests that exceeded rate limits",
            ),
            &["domain", "descriptor"],
        )?;

        let near_limit_requests = CounterVec::new(
            Opts::new(
                "ratelimit_near_limit_requests",
                "Number of requests that are near the rate limit threshold",
            ),
            &["domain", "descriptor"],
        )?;

        let within_limit_requests = CounterVec::new(
            Opts::new(
                "ratelimit_within_limit_requests",
                "Number of requests that are within rate limits",
            ),
            &["domain", "descriptor"],
        )?;

        let shadow_mode_requests = CounterVec::new(
            Opts::new(
                "ratelimit_shadow_mode_requests",
                "Number of requests processed in shadow mode",
            ),
            &["domain", "descriptor"],
        )?;

        let local_cache_hits = Counter::new(
            "ratelimit_local_cache_hits",
            "Number of local cache hits",
        )?;

        let local_cache_misses = Counter::new(
            "ratelimit_local_cache_misses",
            "Number of local cache misses",
        )?;

        let redis_operations = CounterVec::new(
            Opts::new(
                "ratelimit_redis_operations",
                "Number of Redis operations by type",
            ),
            &["operation", "result"],
        )?;

        let redis_operation_duration = HistogramVec::new(
            HistogramOpts::new(
                "ratelimit_redis_operation_duration_seconds",
                "Duration of Redis operations in seconds",
            ),
            &["operation"],
        )?;

        let redis_connection_active = GaugeVec::new(
            Opts::new(
                "ratelimit_redis_connections_active",
                "Number of active Redis connections",
            ),
            &["instance"],
        )?;

        let config_load_success = Counter::new(
            "ratelimit_config_load_success",
            "Number of successful configuration loads",
        )?;

        let config_load_error = Counter::new(
            "ratelimit_config_load_error",
            "Number of failed configuration loads",
        )?;

        let request_duration = Histogram::with_opts(HistogramOpts::new(
            "ratelimit_request_duration_seconds",
            "Duration of rate limit requests in seconds",
        ))?;

        // Register all metrics
        registry.register(Box::new(total_requests.clone()))?;
        registry.register(Box::new(over_limit_requests.clone()))?;
        registry.register(Box::new(near_limit_requests.clone()))?;
        registry.register(Box::new(within_limit_requests.clone()))?;
        registry.register(Box::new(shadow_mode_requests.clone()))?;
        registry.register(Box::new(local_cache_hits.clone()))?;
        registry.register(Box::new(local_cache_misses.clone()))?;
        registry.register(Box::new(redis_operations.clone()))?;
        registry.register(Box::new(redis_operation_duration.clone()))?;
        registry.register(Box::new(redis_connection_active.clone()))?;
        registry.register(Box::new(config_load_success.clone()))?;
        registry.register(Box::new(config_load_error.clone()))?;
        registry.register(Box::new(request_duration.clone()))?;

        Ok(Self {
            registry,
            total_requests,
            over_limit_requests,
            near_limit_requests,
            within_limit_requests,
            shadow_mode_requests,
            local_cache_hits,
            local_cache_misses,
            redis_operations,
            redis_operation_duration,
            redis_connection_active,
            config_load_success,
            config_load_error,
            request_duration,
        })
    }

    /// Get the Prometheus registry for this metrics instance
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Record a total request
    pub fn record_total_request(&self, domain: &str, descriptor: &str) {
        self.total_requests.with_label_values(&[domain, descriptor]).inc();
    }

    /// Record an over-limit request
    pub fn record_over_limit_request(&self, domain: &str, descriptor: &str) {
        self.over_limit_requests.with_label_values(&[domain, descriptor]).inc();
    }

    /// Record a near-limit request
    pub fn record_near_limit_request(&self, domain: &str, descriptor: &str) {
        self.near_limit_requests.with_label_values(&[domain, descriptor]).inc();
    }

    /// Record a within-limit request
    pub fn record_within_limit_request(&self, domain: &str, descriptor: &str) {
        self.within_limit_requests.with_label_values(&[domain, descriptor]).inc();
    }

    /// Record a shadow-mode request
    pub fn record_shadow_mode_request(&self, domain: &str, descriptor: &str) {
        self.shadow_mode_requests.with_label_values(&[domain, descriptor]).inc();
    }

    /// Record a local cache hit
    pub fn record_local_cache_hit(&self) {
        self.local_cache_hits.inc();
    }

    /// Record a local cache miss
    pub fn record_local_cache_miss(&self) {
        self.local_cache_misses.inc();
    }

    /// Record a Redis operation
    pub fn record_redis_operation(&self, operation: &str, result: &str) {
        self.redis_operations.with_label_values(&[operation, result]).inc();
    }

    /// Record Redis operation duration
    pub fn record_redis_operation_duration(&self, operation: &str, duration_seconds: f64) {
        self.redis_operation_duration
            .with_label_values(&[operation])
            .observe(duration_seconds);
    }

    /// Set active Redis connections
    pub fn set_redis_connections_active(&self, instance: &str, count: f64) {
        self.redis_connection_active.with_label_values(&[instance]).set(count);
    }

    /// Record successful configuration load
    pub fn record_config_load_success(&self) {
        self.config_load_success.inc();
    }

    /// Record failed configuration load
    pub fn record_config_load_error(&self) {
        self.config_load_error.inc();
    }

    /// Record request duration
    pub fn record_request_duration(&self, duration_seconds: f64) {
        self.request_duration.observe(duration_seconds);
    }

    /// Create a timer for measuring request duration
    pub fn start_request_timer(&self) -> prometheus::HistogramTimer {
        self.request_duration.start_timer()
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new().expect("Failed to create default metrics")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = Metrics::new().unwrap();
        
        // Test that we can record metrics without panicking
        metrics.record_total_request("test_domain", "test_descriptor");
        metrics.record_over_limit_request("test_domain", "test_descriptor");
        metrics.record_local_cache_hit();
        metrics.record_redis_operation("incr", "success");
        metrics.record_config_load_success();
        
        // Test timer
        let _timer = metrics.start_request_timer();
    }

    #[test]
    fn test_metrics_gathering() {
        let metrics = Metrics::new().unwrap();
        
        // Record some metrics
        metrics.record_total_request("test", "desc");
        metrics.record_over_limit_request("test", "desc");
        
        // Gather metrics
        let families = metrics.registry().gather();
        assert!(!families.is_empty());
        
        // Find our metrics
        let total_requests_found = families.iter().any(|f| f.get_name() == "ratelimit_total_requests");
        assert!(total_requests_found);
    }
}