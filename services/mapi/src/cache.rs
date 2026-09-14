use std::future::Future;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, RwLock};

struct CacheEntry<T> {
    value: T,
    expires_at: Instant,
}

/// A single-entry, single-flight, time-to-live cache that caches both successes and,
/// briefly, the most recent failure.
///
/// On a hit within `ttl`, returns a clone of the cached value with no refresh. On a
/// miss/expiry, callers serialize through `refresh_lock` so only one runs `refresh` at
/// a time; the rest then read the freshly stored value, or — if that refresh failed and
/// is still within `failure_ttl` — return a clone of the leader's *actual* error, without
/// issuing their own backend call. This preserves the precise error class (500/503/504)
/// for every caller while collapsing the thundering herd during an outage, and briefly
/// rate-limits retries.
///
/// The failure slot is a negative cache: for up to `failure_ttl` after a failed refresh,
/// *every* caller (not only the concurrent herd) is served that error before any retry —
/// so right after the backend recovers there is a sub-`failure_ttl` window where the last
/// error is still returned. Keep `failure_ttl` small.
///
/// The cached error is whatever the `refresh` closure returned, so this throttles *any*
/// dependency the closure touches, not only ogmios — e.g. for the timestamped routes the
/// closure also queries dbsync for `last_updated`, so a dbsync failure is negative-cached
/// (and its error class preserved) just like an ogmios one.
///
/// If a leader is cancelled mid-refresh (e.g. client disconnect) it stores nothing, so the
/// next waiter simply becomes the new leader and retries; warming therefore depends on some
/// leader's request outliving the refresh.
pub struct TtlCache<T, E> {
    ttl: Duration,
    failure_ttl: Duration,
    state: RwLock<Option<CacheEntry<T>>>,
    last_error: RwLock<Option<CacheEntry<E>>>,
    refresh_lock: Mutex<()>,
}

impl<T: Clone, E: Clone> TtlCache<T, E> {
    pub fn new(ttl: Duration, failure_ttl: Duration) -> Self {
        Self {
            ttl,
            failure_ttl,
            state: RwLock::new(None),
            last_error: RwLock::new(None),
            refresh_lock: Mutex::new(()),
        }
    }

    async fn fresh(&self) -> Option<T> {
        let guard = self.state.read().await;
        match &*guard {
            Some(entry) if entry.expires_at > Instant::now() => Some(entry.value.clone()),
            _ => None,
        }
    }

    async fn recent_error(&self) -> Option<E> {
        let guard = self.last_error.read().await;
        match &*guard {
            Some(entry) if entry.expires_at > Instant::now() => Some(entry.value.clone()),
            _ => None,
        }
    }

    /// Return a fresh cached value, or refresh via `refresh` (single-flight) on miss/expiry.
    ///
    /// Followers (callers that arrive while a refresh is in flight) wait out the leader and
    /// then return either its stored value or a clone of its actual error — never a synthetic
    /// substitute, and never their own backend call (within `failure_ttl`).
    pub async fn get_or_refresh<F, Fut>(&self, refresh: F) -> Result<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        if let Some(value) = self.fresh().await {
            return Ok(value);
        }
        // Serve a still-fresh negative-cache hit without taking `refresh_lock`, so a burst of
        // callers within `failure_ttl` doesn't queue on the mutex just to read a cached error.
        if let Some(error) = self.recent_error().await {
            return Err(error);
        }

        // Serialize misses: only the lock holder runs `refresh`. Re-check value and recent
        // error while holding the lock so a just-finished leader's outcome is observed.
        let _guard = self.refresh_lock.lock().await;
        if let Some(value) = self.fresh().await {
            return Ok(value);
        }
        if let Some(error) = self.recent_error().await {
            return Err(error);
        }

        match refresh().await {
            Ok(value) => {
                {
                    let mut state = self.state.write().await;
                    *state = Some(CacheEntry {
                        value: value.clone(),
                        expires_at: Instant::now() + self.ttl,
                    });
                }
                *self.last_error.write().await = None;
                Ok(value)
            }
            Err(error) => {
                *self.last_error.write().await = Some(CacheEntry {
                    value: error.clone(),
                    expires_at: Instant::now() + self.failure_ttl,
                });
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    const LONG: Duration = Duration::from_secs(60);

    #[tokio::test]
    async fn miss_then_hit_refreshes_once() {
        let calls = Arc::new(AtomicU32::new(0));
        let cache: TtlCache<u32, Infallible> = TtlCache::new(LONG, LONG);

        let refresh = || {
            let calls = calls.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok::<u32, Infallible>(99)
            }
        };

        assert_eq!(cache.get_or_refresh(refresh).await.unwrap(), 99);

        let refresh2 = || {
            let calls = calls.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok::<u32, Infallible>(99)
            }
        };
        assert_eq!(cache.get_or_refresh(refresh2).await.unwrap(), 99);
        assert_eq!(calls.load(Ordering::SeqCst), 1); // second call was a hit
    }

    #[tokio::test]
    async fn expiry_triggers_refresh() {
        let calls = Arc::new(AtomicU32::new(0));
        let cache: TtlCache<u32, Infallible> = TtlCache::new(Duration::from_millis(50), LONG);

        let mk = || {
            let calls = calls.clone();
            async move {
                let n = calls.fetch_add(1, Ordering::SeqCst);
                Ok::<u32, Infallible>(n)
            }
        };

        let _ = cache.get_or_refresh(mk).await.unwrap();
        tokio::time::sleep(Duration::from_millis(70)).await;

        let mk2 = || {
            let calls = calls.clone();
            async move {
                let n = calls.fetch_add(1, Ordering::SeqCst);
                Ok::<u32, Infallible>(n)
            }
        };
        let _ = cache.get_or_refresh(mk2).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn single_flight_under_concurrency() {
        let calls = Arc::new(AtomicU32::new(0));
        let cache: Arc<TtlCache<u32, Infallible>> = Arc::new(TtlCache::new(LONG, LONG));

        let mut handles = Vec::new();
        for _ in 0..20 {
            let cache = cache.clone();
            let calls = calls.clone();
            handles.push(tokio::spawn(async move {
                cache
                    .get_or_refresh(|| {
                        let calls = calls.clone();
                        async move {
                            calls.fetch_add(1, Ordering::SeqCst);
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            Ok::<u32, Infallible>(1)
                        }
                    })
                    .await
                    .unwrap()
            }));
        }
        for h in handles {
            assert_eq!(h.await.unwrap(), 1);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_refresh_is_negative_cached_then_recovers() {
        let calls = Arc::new(AtomicU32::new(0));
        let cache: TtlCache<u32, &str> = TtlCache::new(LONG, Duration::from_millis(50));

        // First refresh fails and is briefly negative-cached.
        let err: Result<u32, &str> = cache.get_or_refresh(|| async { Err("boom") }).await;
        assert_eq!(err, Err("boom"));

        // Within failure_ttl, the cached error is served without running the closure.
        let c = calls.clone();
        let served: Result<u32, &str> = cache
            .get_or_refresh(|| {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok(5)
                }
            })
            .await;
        assert_eq!(served, Err("boom"));
        assert_eq!(calls.load(Ordering::SeqCst), 0); // closure never ran

        // After failure_ttl lapses, the cache recovers (a failure does not poison it).
        tokio::time::sleep(Duration::from_millis(70)).await;
        let ok: Result<u32, &str> = cache.get_or_refresh(|| async { Ok(5) }).await;
        assert_eq!(ok, Ok(5));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn negative_cache_hit_is_lock_free_under_burst() {
        // Exercises the pre-lock `recent_error()` early return: once an error is negative-cached,
        // a concurrent burst of callers within `failure_ttl` must all be served that error
        // without taking `refresh_lock` and without ever running their refresh closures.
        let calls = Arc::new(AtomicU32::new(0));
        let cache: Arc<TtlCache<u32, &str>> = Arc::new(TtlCache::new(LONG, LONG));

        // Prime the negative cache with a failed refresh.
        let primed: Result<u32, &str> = cache.get_or_refresh(|| async { Err("boom") }).await;
        assert_eq!(primed, Err("boom"));

        // A burst of contended callers all read the cached error; none run the closure.
        let mut handles = Vec::new();
        for _ in 0..20 {
            let cache = cache.clone();
            let calls = calls.clone();
            handles.push(tokio::spawn(async move {
                cache
                    .get_or_refresh(|| {
                        let calls = calls.clone();
                        async move {
                            calls.fetch_add(1, Ordering::SeqCst);
                            Ok::<u32, &str>(5)
                        }
                    })
                    .await
            }));
        }
        for h in handles {
            assert_eq!(h.await.unwrap(), Err("boom"));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0); // no closure ran behind the lock
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_failure_is_single_flight() {
        let calls = Arc::new(AtomicU32::new(0));
        let cache: Arc<TtlCache<u32, &str>> = Arc::new(TtlCache::new(LONG, LONG));

        let mut handles = Vec::new();
        for _ in 0..20 {
            let cache = cache.clone();
            let calls = calls.clone();
            handles.push(tokio::spawn(async move {
                cache
                    .get_or_refresh(|| {
                        let calls = calls.clone();
                        async move {
                            calls.fetch_add(1, Ordering::SeqCst);
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            Err::<u32, &str>("boom")
                        }
                    })
                    .await
            }));
        }
        for h in handles {
            assert!(h.await.unwrap().is_err());
        }
        // Only the leader hit the backend; the other 19 followers read the cached error.
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn followers_receive_leaders_exact_error() {
        // A stand-in for an HTTP status: every follower must observe the leader's actual
        // error value (e.g. 504), not a synthetic substitute.
        let cache: Arc<TtlCache<u32, u16>> = Arc::new(TtlCache::new(LONG, LONG));

        let mut handles = Vec::new();
        for _ in 0..20 {
            let cache = cache.clone();
            handles.push(tokio::spawn(async move {
                cache
                    .get_or_refresh(|| async {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        Err::<u32, u16>(504)
                    })
                    .await
            }));
        }
        for h in handles {
            assert_eq!(h.await.unwrap(), Err(504));
        }
    }
}
