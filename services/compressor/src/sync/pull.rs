use std::collections::VecDeque;

use gasket::framework::*;
use pallas::ledger::traverse::MultiEraBlock;
use tracing::{debug, info};

use pallas::crypto::hash::Hash;
// use pallas::ledger::traverse::MultiEraHeader;
use pallas::network::facades::NodeClient;
use pallas::network::miniprotocols::chainsync::{self, BlockContent, NextResponse};
use pallas::network::miniprotocols::Point;

use super::model::PullEvent;

const HARDCODED_BREADCRUMBS: usize = 20;

#[derive(Clone)]
pub enum Intersection {
    #[allow(dead_code)]
    Tip,
    Origin,
    Breadcrumbs(VecDeque<Point>),
}

impl Intersection {
    pub fn add_breadcrumb(&mut self, slot: u64, hash: &[u8]) {
        let point = Point::Specific(slot, Vec::from(hash));

        match self {
            Intersection::Tip => {
                *self = Intersection::Breadcrumbs(VecDeque::from(vec![point]));
            }
            Intersection::Origin => {
                *self = Intersection::Breadcrumbs(VecDeque::from(vec![point]));
            }
            Intersection::Breadcrumbs(x) => {
                x.push_front(point);

                if x.len() > HARDCODED_BREADCRUMBS {
                    x.pop_back();
                }
            }
        }
    }
}

impl FromIterator<(u64, Hash<32>)> for Intersection {
    fn from_iter<T: IntoIterator<Item = (u64, Hash<32>)>>(iter: T) -> Self {
        let points: VecDeque<_> = iter
            .into_iter()
            .map(|(slot, hash)| Point::Specific(slot, hash.to_vec()))
            .collect();

        if points.is_empty() {
            Intersection::Origin
        } else {
            Intersection::Breadcrumbs(points)
        }
    }
}

// fn to_traverse(header: &HeaderContent) -> Result<MultiEraHeader<'_>, WorkerError> {
//     let out = match header.byron_prefix {
//         Some((subtag, _)) => MultiEraHeader::decode(header.variant, Some(subtag), &header.cbor),
//         None => MultiEraHeader::decode(header.variant, None, &header.cbor),
//     };

//     out.or_panic()
// }

pub type DownstreamPort = gasket::messaging::tokio::OutputPort<PullEvent>;

async fn intersect(peer: &mut NodeClient, intersection: &Intersection) -> Result<(), WorkerError> {
    let chainsync = peer.chainsync();

    let intersect = match intersection {
        Intersection::Origin => {
            info!("intersecting origin");
            chainsync.intersect_origin().await.or_restart()?.into()
        }
        Intersection::Tip => {
            info!("intersecting tip");
            chainsync.intersect_tip().await.or_restart()?.into()
        }
        Intersection::Breadcrumbs(points) => {
            info!("intersecting breadcrumbs");
            let (point, _) = chainsync
                .find_intersect(points.clone().into())
                .await
                .or_restart()?;
            point
        }
    };

    info!(?intersect, "intersected");

    Ok(())
}

pub struct Worker {
    peer_session: NodeClient,
}

impl Worker {
    async fn send(&mut self, stage: &mut Stage, event: PullEvent) -> Result<(), WorkerError> {
        stage
            .downstream
            .send(event.clone().into())
            .await
            .or_panic()?;

        stage
            .health_downstream
            .send(event.into())
            .await
            .or_panic()?;

        Ok(())
    }

    async fn process_next(
        &mut self,
        stage: &mut Stage,
        next: &NextResponse<BlockContent>,
    ) -> Result<(), WorkerError> {
        match next {
            NextResponse::RollForward(cbor, tip) => {
                // let header = to_traverse(header).or_panic()?;
                let block = MultiEraBlock::decode(&cbor).or_panic()?;
                let slot = block.slot();
                let hash = block.hash();
                let height = block.number();

                debug!(height, slot, %hash, "chain sync roll forward");

                // let block = self
                //     .peer_session
                //     .blockfetch()
                //     .fetch_single(Point::Specific(slot, hash.to_vec()))
                //     .await
                //     .or_retry()?;

                self.send(
                    stage,
                    PullEvent::RollForward(slot, hash, cbor.to_vec(), tip.clone()).into(),
                )
                .await
                .or_panic()?;

                stage.intersection.add_breadcrumb(slot, hash.as_ref());

                if height % 100 == 0 {
                    let tip_slot = tip.0.slot_or_default();
                    let slot_pct = (slot * 100) / tip_slot.max(1);
                    let height_pct = (height * 100) / tip.1.max(1);

                    info!(
                        "sync status: ({slot_pct}% by absolute slot, {height_pct}% by block height)"
                    );
                }

                stage.chain_tip.set(tip.0.slot_or_default() as i64);

                Ok(())
            }
            chainsync::NextResponse::RollBackward(point, tip) => {
                match point {
                    Point::Origin => {
                        info!("rollback to origin");

                        self.send(stage, PullEvent::RollBackOrigin(tip.clone()).into())
                            .await
                            .or_panic()?;
                    }
                    Point::Specific(slot, hash) => {
                        info!(slot, ?hash, "rollback to point");

                        let hash: [u8; 32] = hash[0..32].try_into().unwrap();

                        self.send(
                            stage,
                            PullEvent::RollBack(*slot, hash.into(), tip.clone()).into(),
                        )
                        .await
                        .or_panic()?;

                        stage.intersection.add_breadcrumb(*slot, &hash);
                    }
                };

                stage.chain_tip.set(tip.0.slot_or_default() as i64);

                Ok(())
            }
            chainsync::NextResponse::Await => {
                debug!("chain-sync reached the tip of the chain");

                // TODO?

                Ok(())
            }
        }
    }
}

#[async_trait::async_trait(?Send)]
impl gasket::framework::Worker<Stage> for Worker {
    async fn bootstrap(stage: &Stage) -> Result<Self, WorkerError> {
        debug!("connecting to {}", &stage.node_socket);

        let mut peer_session = NodeClient::connect(&stage.node_socket, stage.network_magic)
            .await
            .or_retry()?;

        info!(
            socket = stage.node_socket,
            magic = stage.network_magic,
            "connected to upstream node"
        );

        intersect(&mut peer_session, &stage.intersection).await?;

        let worker = Self { peer_session };

        Ok(worker)
    }

    async fn schedule(
        &mut self,
        _stage: &mut Stage,
    ) -> Result<WorkSchedule<NextResponse<BlockContent>>, WorkerError> {
        let client = self.peer_session.chainsync();

        let next = match client.has_agency() {
            true => {
                debug!("requesting next block");
                client.request_next().await.or_restart()?
            }
            false => {
                debug!("awaiting next block (blocking)");
                client.recv_while_must_reply().await.or_restart()?
            }
        };

        Ok(WorkSchedule::Unit(next))
    }

    async fn execute(
        &mut self,
        unit: &NextResponse<BlockContent>,
        stage: &mut Stage,
    ) -> Result<(), WorkerError> {
        self.process_next(stage, unit).await
    }
}

#[derive(Stage)]
#[stage(name = "peer", unit = "NextResponse<BlockContent>", worker = "Worker")]
pub struct Stage {
    node_socket: String,
    network_magic: u64,
    intersection: Intersection,

    pub downstream: DownstreamPort,
    pub health_downstream: DownstreamPort,

    #[metric]
    block_count: gasket::metrics::Counter,

    #[metric]
    chain_tip: gasket::metrics::Gauge,
}

impl Stage {
    pub fn new(node_socket: String, network_magic: u64, intersection: Intersection) -> Self {
        Self {
            node_socket,
            network_magic,
            intersection,
            downstream: Default::default(),
            health_downstream: Default::default(),
            block_count: Default::default(),
            chain_tip: Default::default(),
        }
    }
}
