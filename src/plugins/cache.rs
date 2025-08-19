use super::plugin::{Plugin, PluginAction};
use crate::config;
use crate::config::new_config::Cache as RCache;
use async_trait::async_trait;
use hickory_proto::op::ResponseCode;
use hickory_proto::{
    op::{Message, Query},
    rr::Record,
};
use hickory_server::server::Request;
use moka::sync::Cache;
use std::time::{Duration, Instant};
#[derive(Clone)]
struct CacheEntry {
    message: Vec<Record>,
    // 我们存储过期时间而不是 TTL，以便于检查
    expires_at: Instant,
}

pub struct CachePlugin {
    config: RCache,
    cache: Cache<Query, CacheEntry>,
}

impl CachePlugin {
    pub fn new(config: RCache) -> Self {
        Self {
            cache: Cache::new(config.max_size),
            config,
        }
    }

    /// 将响应存入缓存的方法
    pub async fn store(&self, query: Query, records: Vec<Record>) {
        if records.len() > 0 {
            // 找到最小的 TTL
            let min_ttl = records
                .iter()
                .map(|r| r.ttl())
                .min()
                .unwrap_or(self.config.min_ttl);

            // 限制 TTL 在我们配置的范围内
            let final_ttl = min_ttl.clamp(self.config.min_ttl, self.config.max_ttl);

            let entry = CacheEntry {
                message: records,
                expires_at: Instant::now() + Duration::from_secs(final_ttl as u64),
            };

            // moka cache 会自动处理过期
            self.cache.insert(query, entry);
        }
    }
}

#[async_trait]
impl Plugin for CachePlugin {
    fn name(&self) -> &'static str {
        "cache"
    }

    async fn handle_request(&self, request: Message) -> Result<PluginAction, ResponseCode> {
        let query = request.query().ok_or(ResponseCode::ServFail)?;
        if let Some(mut entry) = self.cache.get(query) {
            // 检查条目是否已过期
            if entry.expires_at > Instant::now() {
                info!("Cache HIT for {}", query.name());

                return Ok(PluginAction::Response(entry.message));
            } else {
                // 条目已过期，从缓存中移除
                self.cache.invalidate(query);
            }
        }

        info!("Cache MISS for {}", query.name());
        Ok(PluginAction::Continue(request))
    }
}
