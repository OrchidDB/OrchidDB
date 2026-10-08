use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

fn identity() -> Identity<'static> {
    Identity {
        endpoint: "https://catalog.example",
        scope: "tenant",
        graph: "graph",
        revision: None,
        interval: Duration::from_secs(5),
    }
}
fn response(revision: i64) -> Value {
    json!({"manifest": {"protocol_version": 2, "scope": "tenant", "graph": "graph", "revision": revision,
        "graph_version": revision, "description": "", "published_at": "", "objects": {}, "schema": {"tables": []}},
        "principal": {"subject": "reader", "roles": ["read"]}})
}
fn unchanged(revision: i64) -> Value {
    json!({"unchanged": true, "scope": "tenant", "graph": "graph", "revision": revision,
        "principal": {"subject": "reader", "roles": ["updated"]}})
}

#[tokio::test]
async fn warm_queries_make_zero_catalog_requests() {
    let entry = Entry::default();
    let now = Instant::now();
    resolve(&entry, &identity(), false, now, |_| async {
        Ok(response(1))
    })
    .await
    .unwrap();
    for _ in 0..100 {
        let snapshot = resolve(
            &entry,
            &identity(),
            false,
            now + Duration::from_secs(4),
            |_| async { panic!("warm query attempted a catalog request") },
        )
        .await
        .unwrap();
        assert_eq!(snapshot.revision, 1);
    }
}
#[tokio::test]
async fn expired_snapshot_revalidates_without_rebuilding_metadata() {
    let entry = Entry::default();
    let now = Instant::now();
    let first = resolve(&entry, &identity(), false, now, |_| async {
        Ok(response(1))
    })
    .await
    .unwrap();
    let next = resolve(
        &entry,
        &identity(),
        false,
        now + Duration::from_secs(6),
        |known| async move {
            assert_eq!(known, Some(1));
            Ok(unchanged(1))
        },
    )
    .await
    .unwrap();
    assert!(Arc::ptr_eq(
        first.manifest.as_ref().unwrap(),
        next.manifest.as_ref().unwrap()
    ));
    assert_eq!(next.principal.unwrap().roles, ["updated"]);
}
#[tokio::test]
async fn publication_is_visible_after_expiry_or_explicit_refresh() {
    let entry = Entry::default();
    let now = Instant::now();
    resolve(&entry, &identity(), false, now, |_| async {
        Ok(response(1))
    })
    .await
    .unwrap();
    let refreshed = resolve(&entry, &identity(), true, now, |known| async move {
        assert_eq!(known, Some(1));
        Ok(response(2))
    })
    .await
    .unwrap();
    assert_eq!(refreshed.revision, 2);
    let expired = resolve(
        &entry,
        &identity(),
        false,
        now + Duration::from_secs(6),
        |known| async move {
            assert_eq!(known, Some(2));
            Ok(response(3))
        },
    )
    .await
    .unwrap();
    assert_eq!(expired.revision, 3);
}
#[tokio::test]
async fn revocation_and_outages_never_fall_back_to_expired_metadata() {
    for error in [
        "catalog HTTP 403",
        "catalog HTTP 401",
        "catalog transport failed",
    ] {
        let entry = Entry::default();
        let now = Instant::now();
        resolve(&entry, &identity(), false, now, |_| async {
            Ok(response(1))
        })
        .await
        .unwrap();
        assert_eq!(
            resolve(
                &entry,
                &identity(),
                false,
                now + Duration::from_secs(6),
                |_| async { Err(error.into()) }
            )
            .await
            .unwrap_err(),
            error
        );
        assert!(entry.0.lock().await.is_none());
        resolve(
            &entry,
            &identity(),
            false,
            now + Duration::from_secs(7),
            |known| async move {
                assert_eq!(known, None);
                Ok(response(1))
            },
        )
        .await
        .unwrap();
    }
}
#[tokio::test]
async fn pinned_revision_still_revalidates_authorization() {
    let entry = Entry::default();
    let mut identity = identity();
    identity.revision = Some(1);
    let now = Instant::now();
    resolve(&entry, &identity, false, now, |_| async { Ok(response(1)) })
        .await
        .unwrap();
    assert!(
        resolve(
            &entry,
            &identity,
            false,
            now + Duration::from_secs(6),
            |_| async { Ok(response(2)) }
        )
        .await
        .is_err()
    );
    assert!(entry.0.lock().await.is_none());
}
#[tokio::test]
async fn concurrent_cold_queries_share_one_refresh() {
    let entry = Entry::default();
    let count = AtomicUsize::new(0);
    let identity = identity();
    let futures = (0..32).map(|_| {
        resolve(&entry, &identity, false, Instant::now(), |_| async {
            count.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            Ok(response(1))
        })
    });
    for result in futures::future::join_all(futures).await {
        assert_eq!(result.unwrap().revision, 1);
    }
    assert_eq!(count.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn zero_interval_revalidates_every_time() {
    let entry = Entry::default();
    let mut identity = identity();
    identity.interval = Duration::ZERO;
    let now = Instant::now();
    resolve(&entry, &identity, false, now, |_| async { Ok(response(1)) })
        .await
        .unwrap();
    let result = resolve(&entry, &identity, false, now, |known| async move {
        assert_eq!(known, Some(1));
        Ok(unchanged(1))
    })
    .await
    .unwrap();
    assert_eq!(result.principal.unwrap().roles, ["updated"]);
}
#[tokio::test]
async fn mismatched_conditional_response_is_rejected_and_evicted() {
    for field in ["scope", "graph", "revision"] {
        let entry = Entry::default();
        let now = Instant::now();
        resolve(&entry, &identity(), false, now, |_| async {
            Ok(response(1))
        })
        .await
        .unwrap();
        let mut bad = unchanged(1);
        bad[field] = json!("wrong");
        assert!(
            resolve(&entry, &identity(), true, now, |_| async { Ok(bad) })
                .await
                .is_err()
        );
        assert!(entry.0.lock().await.is_none());
    }
}
#[test]
fn cache_identity_isolates_credentials_endpoints_graphs_revisions_and_policies() {
    let identity = identity();
    let first = entry(&identity, b"first credential").unwrap();
    assert!(Arc::ptr_eq(
        &first,
        &entry(&identity, b"first credential").unwrap()
    ));
    assert!(!Arc::ptr_eq(
        &first,
        &entry(&identity, b"rotated credential").unwrap()
    ));
    for other in [
        Identity {
            endpoint: "https://other.example",
            ..self::identity()
        },
        Identity {
            scope: "other",
            ..self::identity()
        },
        Identity {
            graph: "other",
            ..self::identity()
        },
        Identity {
            revision: Some(1),
            ..self::identity()
        },
        Identity {
            interval: Duration::ZERO,
            ..self::identity()
        },
    ] {
        assert!(!Arc::ptr_eq(
            &first,
            &entry(&other, b"first credential").unwrap()
        ));
    }
}
