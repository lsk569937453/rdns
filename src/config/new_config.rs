use serde::Deserialize;
use std::collections::HashMap;

/// 顶层配置结构，对应整个 YAML 文件
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub port: i32,
    pub log_level: LogLevel,
    pub pipeline: Vec<String>,

    // 插件具体配置
    pub client_limiter: Option<ClientLimiterConfig>,
    pub cache: Option<Cache>,
    pub hosts: Option<Hosts>,
    pub blackhole: Option<Blackhole>,
    pub redirect: Option<Redirect>,
    #[serde(rename = "_prefer_ipv4")]
    pub prefer_ipv4: Option<PreferIpv4>,
    pub ecs: Option<Ecs>,
    pub padding: Option<Padding>,
    pub bufsize: Option<Bufsize>,
    pub ttl: Option<Ttl>,
    pub fast_forward: Option<FastForward>,

    // 以下插件在 pipeline 中未启用，但仍为其定义结构
    #[serde(default)] // 如果配置文件中没有，则使用默认值
    pub arbitrary: Option<Arbitrary>,
    #[serde(default)]
    pub reverse_lookup: ReverseLookup,
    #[serde(rename = "_prefer_ipv6")]
    #[serde(default)]
    pub prefer_ipv6: PreferIpv6,
}

/// 日志级别枚举
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// fast_forward: 请求转发插件
#[derive(Debug, Deserialize, Clone)]
pub struct FastForward {
    pub upstreams: Vec<String>,
    pub strategy: ForwardStrategy,
}

/// 转发策略枚举
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ForwardStrategy {
    Random,
    RoundRobin,
    Fastest,
}

/// cache: 缓存插件
#[derive(Debug, Deserialize, Clone)]
pub struct Cache {
    pub backend: CacheBackend,
    pub max_size: u64,
    pub ttl_override: bool,
    pub min_ttl: u32,
    pub max_ttl: u32,
    pub redis: Option<RedisConfig>,
}

/// 缓存后端类型枚举
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum CacheBackend {
    Memory,
    Redis,
}

/// Redis 配置
#[derive(Debug, Deserialize, Clone)]
pub struct RedisConfig {
    pub url: String,
}

/// _prefer_ipv4: IPv4 偏好插件
#[derive(Debug, Deserialize, Default, Clone)]
pub struct PreferIpv4 {}

/// _prefer_ipv6: IPv6 偏好插件
#[derive(Debug, Deserialize, Default, Clone)]
pub struct PreferIpv6 {}

/// ecs: EDNS Client Subnet 插件
#[derive(Debug, Deserialize, Clone)]
pub struct Ecs {
    pub strategy: EcsStrategy,
    pub preset_ip: Option<String>,
    pub subnet_mask_ipv4: u8,
    pub subnet_mask_ipv6: u8,
}

/// ECS 添加策略枚举
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum EcsStrategy {
    Auto,
    Preset,
}

/// hosts: 静态域名记录插件
#[derive(Debug, Deserialize, Clone)]
pub struct Hosts {
    #[serde(default)] // 如果 records 字段不存在，则默认为空的 HashMap
    pub records: HashMap<String, HostRecord>,
}

/// 用于处理 hosts 中可能是单个 IP 或多个 IP 列表的情况
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum HostRecord {
    Single(String),
    Multiple(Vec<String>),
}

/// blackhole: 黑洞/屏蔽插件
#[derive(Debug, Deserialize, Clone)]
pub struct Blackhole {
    pub strategy: BlackholeStrategy,
    pub custom_ips: Option<Vec<String>>,
    #[serde(default)]
    pub domains: Vec<String>,
}

/// 屏蔽策略枚举
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum BlackholeStrategy {
    Drop,
    Empty,
    CustomIp,
}

/// ttl: TTL 修改插件
#[derive(Debug, Deserialize, Clone)]
pub struct Ttl {
    pub rules: HashMap<String, u32>,
}

/// redirect: 请求重定向插件
#[derive(Debug, Deserialize, Clone)]
pub struct Redirect {
    pub rules: HashMap<String, String>,
}

/// padding: EDNS(0) Padding 插件
#[derive(Debug, Deserialize, Clone)]
pub struct Padding {
    pub block_size: u16,
}

/// bufsize: EDNS(0) UDP Buffer Size 修改插件
#[derive(Debug, Deserialize, Clone)]
pub struct Bufsize {
    pub size: u16,
}

/// arbitrary: 高级自定义应答插件
#[derive(Debug, Deserialize, Default, Clone)]
pub struct Arbitrary {
    #[serde(default)]
    pub rules: Vec<ArbitraryRule>,
}

/// 高级自定义应答规则
#[derive(Debug, Deserialize, Clone)]
pub struct ArbitraryRule {
    pub domain: String,
    pub qtype: String,
    pub value: String,
    pub ttl: u32,
}

/// reverse_lookup: 反向查询插件
#[derive(Debug, Deserialize, Default, Clone)]
pub struct ReverseLookup {
    pub http_api: HttpApiConfig,
    pub ptr_lookup: PtrLookupConfig,
}

/// 反向查询的 HTTP API 配置
#[derive(Debug, Deserialize, Default, Clone)]
pub struct HttpApiConfig {
    pub listen_address: String,
}

/// 反向查询的 PTR 查询配置
#[derive(Debug, Deserialize, Default, Clone)]
pub struct PtrLookupConfig {}

/// client_limiter: 客户端请求频率限制插件
#[derive(Debug, Deserialize, Clone)]
pub struct ClientLimiterConfig {
    pub max_qps: u32,
    pub burst_size: u32,
}
