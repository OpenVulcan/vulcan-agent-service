use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// 中文：工具缓存默认最大条目数，用于限制进程内共享缓存的总体容量。
/// English: Default maximum number of cached entries for the process-wide shared tool cache.
pub const DEFAULT_TOOL_CACHE_MAX_ENTRIES: usize = 1000;

/// 中文：工具缓存默认存活时间，单位为秒；未显式指定时使用该值。
/// English: Default cache entry lifetime in seconds, used when callers do not provide an explicit TTL.
pub const DEFAULT_TOOL_CACHE_DEFAULT_TTL_SECS: u64 = 30 * 60;

/// 中文：工具缓存允许的最长存活时间，单位为秒；超过该值会被自动钳制。
/// English: Maximum cache entry lifetime in seconds; larger requested TTL values are clamped to this ceiling.
pub const DEFAULT_TOOL_CACHE_MAX_TTL_SECS: u64 = 30 * 60;

/// 中文：共享工具缓存的运行时配置，控制容量与过期策略。
/// English: Runtime configuration for the shared tool cache, controlling capacity and expiration behavior.
#[derive(Clone, Debug)]
pub struct ToolCacheConfig {
    /// 中文：缓存最大条目数，超出后会按创建顺序淘汰最旧条目。
    /// English: Maximum number of entries; oldest entries are evicted when the cache exceeds this size.
    pub max_entries: usize,
    /// 中文：默认 TTL（秒），调用方未传 TTL 时使用。
    /// English: Default TTL in seconds used when callers omit a TTL.
    pub default_ttl_secs: u64,
    /// 中文：最大 TTL（秒），请求 TTL 会被限制在该范围内。
    /// English: Maximum TTL in seconds; requested TTL values are clamped to this ceiling.
    pub max_ttl_secs: u64,
}

impl Default for ToolCacheConfig {
    fn default() -> Self {
        Self {
            max_entries: DEFAULT_TOOL_CACHE_MAX_ENTRIES,
            default_ttl_secs: DEFAULT_TOOL_CACHE_DEFAULT_TTL_SECS,
            max_ttl_secs: DEFAULT_TOOL_CACHE_MAX_TTL_SECS,
        }
    }
}

/// 中文：单个缓存条目的内部表示，记录归属工具、内容、创建顺序与过期时间。
/// English: Internal representation of one cache entry, recording the owning tool, payload, creation order, and expiration time.
#[derive(Clone, Debug)]
struct ToolCacheEntry {
    /// 中文：写入该条目的工具/技能名称，用于隔离不同工具的缓存空间。
    /// English: Tool or skill name that owns this entry, used to isolate cache namespaces.
    tool_name: String,
    /// 中文：缓存的 JSON 值，会在读取时原样返回给调用方。
    /// English: Cached JSON payload returned to callers as-is on reads.
    value: Value,
    /// 中文：创建序号，用于在容量超限时淘汰最旧条目。
    /// English: Monotonic creation sequence used to evict the oldest entry when capacity is exceeded.
    created_seq: u64,
    /// 中文：条目失效时刻；超过该时间后读取会自动清理。
    /// English: Expiration instant; reads after this moment trigger automatic cleanup.
    expires_at: Instant,
}

/// 中文：共享缓存的可变存储体，受读写锁保护。
/// English: Mutable storage backing the shared cache, protected by a read-write lock.
#[derive(Default)]
struct ToolCacheStore {
    entries: HashMap<String, ToolCacheEntry>,
}

/// 中文：主程序级共享工具缓存，供所有 Lua 技能复用短时分页/状态数据。
/// English: Process-wide shared cache for all Lua skills, intended for short-lived pagination and tool state handoff.
pub struct SharedToolCache {
    store: RwLock<ToolCacheStore>,
    config: ToolCacheConfig,
    counter: AtomicU64,
}

impl SharedToolCache {
    /// 中文：使用指定配置创建共享缓存实例。
    /// English: Create a shared cache instance with the provided configuration.
    pub fn new(config: ToolCacheConfig) -> Self {
        Self {
            store: RwLock::new(ToolCacheStore::default()),
            config,
            counter: AtomicU64::new(1),
        }
    }

    /// 中文：写入一条缓存记录；TTL 为空时使用默认值，超出上限时会自动钳制。
    /// English: Store one cache record; missing TTL falls back to the default and values above the ceiling are clamped.
    pub fn create(&self, tool_name: &str, value: Value, ttl_secs: Option<u64>) -> String {
        let now = Instant::now();
        let ttl = self.resolve_ttl(ttl_secs);
        let cache_id = self.next_cache_id();
        let entry = ToolCacheEntry {
            tool_name: tool_name.to_string(),
            value,
            created_seq: self.counter.fetch_add(1, Ordering::Relaxed),
            expires_at: now + ttl,
        };

        let mut store = self.store.write().expect("tool cache poisoned");
        self.cleanup_expired_locked(&mut store, now);
        store.entries.insert(cache_id.clone(), entry);
        self.enforce_capacity_locked(&mut store);
        cache_id
    }

    /// 中文：按工具名和缓存编号读取缓存；命中但已过期时会自动删除并返回空。
    /// English: Read a cached entry by tool name and cache id; expired hits are removed and returned as empty.
    pub fn get(&self, tool_name: &str, cache_id: &str) -> Option<Value> {
        let now = Instant::now();

        {
            let store = self.store.read().expect("tool cache poisoned");
            if let Some(entry) = store.entries.get(cache_id)
                && entry.tool_name == tool_name
                && entry.expires_at > now
            {
                return Some(entry.value.clone());
            }
        }

        let mut store = self.store.write().expect("tool cache poisoned");
        self.cleanup_expired_locked(&mut store, now);
        match store.entries.get(cache_id) {
            Some(entry) if entry.tool_name == tool_name && entry.expires_at > now => {
                Some(entry.value.clone())
            }
            _ => None,
        }
    }

    /// 中文：删除指定工具名下的缓存条目；返回是否确实删除了条目。
    /// English: Delete one cache entry under the given tool namespace and return whether an entry was actually removed.
    pub fn delete(&self, tool_name: &str, cache_id: &str) -> bool {
        let mut store = self.store.write().expect("tool cache poisoned");
        self.cleanup_expired_locked(&mut store, Instant::now());
        if let Some(entry) = store.entries.get(cache_id)
            && entry.tool_name == tool_name
        {
            store.entries.remove(cache_id);
            return true;
        }
        false
    }

    /// 中文：解析最终 TTL，未传使用默认值，超限后按最大值裁剪，最小保证为 1 秒。
    /// English: Resolve the effective TTL by applying defaulting, clamping to the configured maximum, and enforcing a 1-second minimum.
    fn resolve_ttl(&self, ttl_secs: Option<u64>) -> Duration {
        let requested = ttl_secs.unwrap_or(self.config.default_ttl_secs);
        let clamped = requested.max(1).min(self.config.max_ttl_secs.max(1));
        Duration::from_secs(clamped)
    }

    /// 中文：生成缓存编号，结合时间戳与自增计数以降低碰撞风险。
    /// English: Generate a cache id by combining a timestamp with a monotonic counter to reduce collision risk.
    fn next_cache_id(&self) -> String {
        let unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let seq = self.counter.fetch_add(1, Ordering::Relaxed);
        format!("tc-{}-{}", unix_ms, seq)
    }

    /// 中文：清理所有已过期条目，保证后续读写看到的是当前有效视图。
    /// English: Remove all expired entries so subsequent reads and writes operate on the current valid view.
    fn cleanup_expired_locked(&self, store: &mut ToolCacheStore, now: Instant) {
        store.entries.retain(|_, entry| entry.expires_at > now);
    }

    /// 中文：在缓存超出上限时淘汰最旧条目，直到条目数回落到配置范围内。
    /// English: Evict the oldest entries while the cache is above its configured capacity.
    fn enforce_capacity_locked(&self, store: &mut ToolCacheStore) {
        while store.entries.len() > self.config.max_entries.max(1) {
            let oldest_id = store
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.created_seq)
                .map(|(cache_id, _)| cache_id.clone());
            match oldest_id {
                Some(cache_id) => {
                    store.entries.remove(&cache_id);
                }
                None => break,
            }
        }
    }
}

static GLOBAL_TOOL_CACHE: OnceLock<Arc<SharedToolCache>> = OnceLock::new();

/// 中文：初始化全局共享缓存；通常在主程序启动时执行一次。
/// English: Initialize the global shared cache, typically once during process startup.
pub fn configure_global_tool_cache(config: ToolCacheConfig) {
    let _ = GLOBAL_TOOL_CACHE.set(Arc::new(SharedToolCache::new(config)));
}

/// 中文：获取全局共享缓存；若尚未初始化则使用默认配置惰性创建。
/// English: Get the global shared cache, lazily creating it with default settings if startup did not configure it explicitly.
pub fn global_tool_cache() -> Arc<SharedToolCache> {
    GLOBAL_TOOL_CACHE
        .get_or_init(|| Arc::new(SharedToolCache::new(ToolCacheConfig::default())))
        .clone()
}
