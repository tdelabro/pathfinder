use anyhow::Context;
use futures::{Stream, StreamExt};
use p2p::client::peer_agnostic::Client as P2PClient;
use p2p::PeerData;
use pathfinder_common::{BlockHash, BlockNumber, SignedBlockHeader};
use pathfinder_common::{BlockHash, BlockHeader, BlockNumber, SignedBlockHeader, TransactionHash};
use tokio_stream::wrappers::ReceiverStream;

use crate::sync::error::SyncError2;
use crate::sync::headers;
use crate::sync::stream::{SyncStream, SyncStreamExt};

pub struct Sync<L> {
    latest: L,
    p2p: P2PClient,
    storage: pathfinder_storage::Storage,
}

impl<L> Sync<L>
where
    L: Stream<Item = (BlockNumber, BlockHash)> + Clone + Send + 'static,
{
    pub async fn run(self) -> Result<(), SyncError2> {
        let latest_local = tokio::task::spawn_blocking({
            let storage = self.storage.clone();
            move || {
                let mut db = storage
                    .connection()
                    .context("Creating database connection")?;
                let db = db.transaction().context("Creating database transaction")?;

                db.block_id(pathfinder_storage::BlockId::Latest)
            }
        })
        .await
        .context("Joining blocking task")?
        .context("Querying latest local block ID")?;

        let mut headers = HeaderSource {
            p2p: self.p2p.clone(),
            latest_onchain: self.latest.clone(),
            start: latest_local.unwrap_or_default().0,
        }
        .spawn()
        .map_stage(
            headers::ForwardContinuityCheck::new(latest_local.clone()),
            100,
        )
        .map_stage(headers::VerifyHash {}, 100);

        let HeaderFanout { headers, events } = HeaderFanout::from_source(headers, 10);

        todo!()
    }
}

struct HeaderSource<L> {
    p2p: P2PClient,
    latest_onchain: L,
    start: BlockNumber,
}

impl<L> HeaderSource<L>
where
    L: Stream<Item = (BlockNumber, BlockHash)> + Send + 'static,
{
    fn spawn(self) -> impl SyncStream<SignedBlockHeader> {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        let Self {
            p2p,
            latest_onchain,
            mut start,
        } = self;

        tokio::spawn(async move {
            let mut latest_onchain = Box::pin(latest_onchain);
            while let Some(latest_onchain) = latest_onchain.next().await {
                // Ignore reorgs for now. Unsure how to handle this properly.

                let mut headers =
                    Box::pin(p2p.clone().header_stream(start, latest_onchain.0, false));

                while let Some(header) = headers.next().await {
                    start = header.data.header.number + 1;

                    let header = PeerData::new(header.peer, Ok(header.data));
                    if tx.send(header).await.is_err() {
                        return;
                    }
                }
            }
        });

        ReceiverStream::new(rx)
    }
}

struct HeaderFanout {
    headers: Box<dyn SyncStream<SignedBlockHeader>>,
    events: Box<dyn Stream<Item = BlockHeader>>,
}

impl HeaderFanout {
    fn from_source<S>(source: S, buffer: usize) -> Self
    where
        S: SyncStream<SignedBlockHeader> + Send + 'static,
    {
        let (h_tx, h_rx) = tokio::sync::mpsc::channel(buffer);
        let (e_tx, e_rx) = tokio::sync::mpsc::channel(buffer);

        tokio::spawn(async move {
            let mut source = Box::pin(source);
            while let Some(signed_header) = source.next().await {
                let is_err = signed_header.data.is_err();

                if h_tx.send(signed_header.clone()).await.is_err() || is_err {
                    return;
                }

                let header = signed_header
                    .data
                    .expect("Error case already handled")
                    .header;

                if e_tx.send(header).await.is_err() {
                    return;
                }
            }
        });

        Self {
            headers: Box::new(ReceiverStream::new(h_rx)),
            events: Box::new(ReceiverStream::new(e_rx)),
        }
    }
}
