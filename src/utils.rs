use chrono::{DateTime, Utc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Time utilities for rate limiting calculations
pub struct TimeSource {
    _private: (),
}

impl TimeSource {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Get the current Unix timestamp in seconds
    pub fn unix_now(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs() as i64
    }

    /// Get the current time as a DateTime<Utc>
    pub fn utc_now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

impl Default for TimeSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Rate limit time units
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Second,
    Minute,
    Hour,
    Day,
}

impl Unit {
    /// Convert unit to seconds (duration)
    pub fn to_seconds(self) -> u64 {
        match self {
            Unit::Second => 1,
            Unit::Minute => 60,
            Unit::Hour => 3600,
            Unit::Day => 86400,
        }
    }

    /// Get the divisor for time window calculations
    pub fn to_divisor(self) -> i64 {
        self.to_seconds() as i64
    }

    /// Check if this is a per-second unit
    pub fn is_per_second(self) -> bool {
        matches!(self, Unit::Second)
    }
}

// Proto conversion implementations would go here when using actual protobuf generation
// For now, we'll use simple integer mappings in the service layer

/// Calculate when the rate limit window will reset
pub fn calculate_reset(unit: &Unit, time_source: &TimeSource) -> Duration {
    let now = time_source.unix_now();
    let window_size = unit.to_divisor();
    let current_window = now / window_size;
    let next_window_start = (current_window + 1) * window_size;
    let seconds_until_reset = next_window_start - now;
    
    Duration::from_secs(seconds_until_reset as u64)
}

/// Generate cache key for a rate limit
pub fn generate_cache_key(
    domain: &str,
    descriptors: &[(&str, &str)],
    unit: Unit,
    time_source: &TimeSource,
) -> String {
    let now = time_source.unix_now();
    let window_size = unit.to_divisor();
    let current_window = now / window_size;

    let mut key_parts = vec![domain.to_string()];
    
    for (key, value) in descriptors {
        if value.is_empty() {
            key_parts.push(key.to_string());
        } else {
            key_parts.push(format!("{}_{}", key, value));
        }
    }
    
    key_parts.push(current_window.to_string());
    
    key_parts.join(":")
}

/// Extract hits addend from request, defaulting to 1
pub fn get_hits_addend(hits_addend: u32) -> u64 {
    if hits_addend == 0 { 1 } else { hits_addend as u64 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unit_conversions() {
        assert_eq!(Unit::Second.to_seconds(), 1);
        assert_eq!(Unit::Minute.to_seconds(), 60);
        assert_eq!(Unit::Hour.to_seconds(), 3600);
        assert_eq!(Unit::Day.to_seconds(), 86400);
    }

    #[test]
    fn test_cache_key_generation() {
        let time_source = TimeSource::new();
        let descriptors = vec![("database", "users"), ("action", "read")];
        
        let key = generate_cache_key("mongo", &descriptors, Unit::Second, &time_source);
        assert!(key.starts_with("mongo:database_users:action_read:"));
    }

    #[test]
    fn test_hits_addend() {
        assert_eq!(get_hits_addend(0), 1);
        assert_eq!(get_hits_addend(5), 5);
    }
}