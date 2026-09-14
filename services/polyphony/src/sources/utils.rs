use pallas::network::miniprotocols::Point;
use tonic::{transport::Channel, Code};

use crate::{
    crosscut, sources::compressor::compressor_api::PageBlocksWithCtxRequest, storage, Error,
};

use super::compressor::compressor_api::{sync_service_client::SyncServiceClient, BlockRef};

pub static IMMUTABLE_AFTER_SLOTS: u64 = 129600;

pub fn point_to_block_ref(point: Point) -> Option<BlockRef> {
    match point {
        Point::Origin => None,
        Point::Specific(slot, hash) => Some(BlockRef { slot, hash }),
    }
}

pub fn block_ref_to_point(block_ref: Option<BlockRef>) -> Point {
    match block_ref {
        None => Point::Origin,
        Some(BlockRef { slot, hash }) => Point::Specific(slot, hash),
    }
}

pub async fn try_intersect_with_last_point_or_config(
    intersect: &crosscut::IntersectConfig,
    cursor: &mut storage::Cursor,
    client: &mut SyncServiceClient<Channel>,
) -> Result<Point, crate::Error> {
    match cursor.last_point().await? {
        Some(point) => {
            tracing::info!("found existing cursor in storage plugin: {:?}", point);

            let res = client
                .page_blocks_with_context(PageBlocksWithCtxRequest {
                    cursor: point_to_block_ref(point.clone()),
                    max_items: 1,
                })
                .await;

            match res {
                Ok(_) => {
                    tracing::info!("found intersect with cursor: {:?}", point);
                    return Ok(point);
                }
                Err(e) if e.code() == Code::NotFound => {
                    tracing::error!("could not intersect using cursor: {:?}", point);
                    return Err(Error::IntersectNotFound);
                }
                Err(e) => return Err(Error::source(e)),
            }
        }
        None => tracing::info!("no cursor found in storage plugin"),
    };

    match &intersect {
        crosscut::IntersectConfig::Origin => {
            tracing::info!("using origin as intersect");

            Ok(Point::Origin)
        }
        crosscut::IntersectConfig::Tip => {
            let res = client
                .page_blocks_with_context(PageBlocksWithCtxRequest {
                    cursor: None,
                    max_items: 1,
                })
                .await
                .map_err(Error::source)?;

            let tip = res.into_inner().chain_tip;

            tracing::info!("using source tip as intersect: {:?}", tip);

            if let Some(BlockRef { slot, hash }) = tip {
                Ok(Point::Specific(slot, hash))
            } else {
                Ok(Point::Origin)
            }
        }
        crosscut::IntersectConfig::Point(_, _) => {
            let point = intersect.get_point().expect("point value");

            let res = client
                .page_blocks_with_context(PageBlocksWithCtxRequest {
                    cursor: point_to_block_ref(point.clone()),
                    max_items: 1,
                })
                .await;

            match res {
                Ok(_) => {
                    tracing::info!("found intersect with config point: {:?}", point);
                    Ok(point)
                }
                Err(e) if e.code() == Code::NotFound => {
                    tracing::error!("could not intersect using config point: {:?}", point);
                    Err(Error::IntersectNotFound)
                }
                Err(e) => Err(Error::source(e)),
            }
        }
    }
}

pub async fn try_intersect_with_rollback_buf(
    cursor: &mut storage::Cursor,
    client: &mut SyncServiceClient<Channel>,
) -> Result<Option<Point>, crate::Error> {
    match cursor.fetch_persistent_buffer().await? {
        Some(buf) => {
            // sort our persistent buf points newest to oldest
            let points: Vec<Point> = buf.into_iter().map(|v| v.point).rev().collect();

            // A buffer left over from an earlier mutable episode can be far
            // behind the cursor (the instance later fell back to immutable
            // paging and advanced past it — e.g. while chasing a still-syncing
            // upstream). Intersecting at such stale points would re-process a
            // large stretch of already-committed history, so prefer the cursor
            // instead.
            if let Some(cursor_point) = cursor.last_point().await? {
                let newest_buffered = points.first().map(|p| p.slot_or_default()).unwrap_or(0);

                if newest_buffered < cursor_point.slot_or_default() {
                    tracing::info!(
                        "persistent rollback buffer (newest slot {}) is behind the cursor ({}); ignoring it",
                        newest_buffered,
                        cursor_point.slot_or_default()
                    );
                    return Ok(None);
                }
            }

            tracing::info!(
                "found persistent rollback buffer in storage plugin with points: {:?}",
                points
            );

            // for each point, keep trying to request blocks from that point
            // until we get a success, which means we intersected with the chain
            // at that point
            for point in points {
                let res = client
                    .page_blocks_with_context(PageBlocksWithCtxRequest {
                        cursor: point_to_block_ref(point.clone()),
                        max_items: 1,
                    })
                    .await;

                match res {
                    Ok(_) => {
                        tracing::info!("found intersect with rb buffer: {:?}", point);
                        return Ok(Some(point));
                    }
                    Err(e) if e.code() == Code::NotFound => (),
                    Err(e) => return Err(Error::source(e)),
                }
            }

            Err(Error::RollbackOutOfRange(
                "no intersect found with persistent rb buffer (maybe upstream catching up?)".into(),
            ))
        }
        None => {
            tracing::info!("no persistent rollback buffer found");

            Ok(None)
        }
    }
}
