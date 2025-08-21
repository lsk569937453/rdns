use crate::config::new_config::ClientLimiterConfig;
pub struct ClientLimiter {
    config: ClientLimiterConfig,
}
impl ClientLimiter {
    pub fn new(config: ClientLimiterConfig) -> Self {
        Self { config }
    }
}
