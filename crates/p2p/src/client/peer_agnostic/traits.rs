use futures::{Future, Stream};
use libp2p::PeerId;
use pathfinder_common::event::Event;
use pathfinder_common::state_update::StateUpdateData;
use pathfinder_common::transaction::TransactionVariant;
use pathfinder_common::{BlockNumber, StateDiffCommitment, TransactionCommitment, TransactionHash};

use crate::client::types::{
    ClassDefinition,
    ClassDefinitionsError,
    EventsForBlockByTransaction,
    IncorrectStateDiffCount,
    Receipt,
    SignedBlockHeader,
    UnverifiedStateUpdateData,
    UnverifiedTransactionData,
};
use crate::PeerData;

pub trait HeaderStream {
    fn header_stream(
        self,
        start: BlockNumber,
        stop: BlockNumber,
        reverse: bool,
    ) -> impl Stream<Item = PeerData<SignedBlockHeader>> + Send;
}

pub trait TransactionStream {
    fn transaction_stream(
        self,
        start: BlockNumber,
        stop: BlockNumber,
        transaction_counts_and_commitments_stream: impl Stream<
            Item = anyhow::Result<(usize, TransactionCommitment)>,
        >,
    ) -> impl Stream<
        Item = Result<PeerData<(UnverifiedTransactionData, BlockNumber)>, PeerData<anyhow::Error>>,
    >;
}

pub trait StateDiffStream {
    /// ### Important
    ///
    /// Contract class updates are by default set to
    /// `ContractClassUpdate::Deploy` but __the caller is responsible for
    /// determining if the class was really deployed or replaced__.
    fn state_diff_stream(
        self,
        start: BlockNumber,
        stop: BlockNumber,
        state_diff_length_and_commitment_stream: impl Stream<
            Item = anyhow::Result<(usize, StateDiffCommitment)>,
        >,
    ) -> impl Stream<
        Item = Result<PeerData<(UnverifiedStateUpdateData, BlockNumber)>, PeerData<anyhow::Error>>,
    >;
}

pub trait ClassStream {
    fn class_stream(
        self,
        start: BlockNumber,
        stop: BlockNumber,
        declared_class_counts_stream: impl Stream<Item = anyhow::Result<usize>>,
    ) -> impl Stream<Item = Result<PeerData<ClassDefinition>, PeerData<anyhow::Error>>>;
}

pub trait EventStream {
    /// ### Important
    ///
    /// Events are grouped by block and by transaction. The order of flattened
    /// events in a block is guaranteed to be correct because the event
    /// commitment is part of block hash. However the number of events per
    /// transaction for __pre 0.13.2__ Starknet blocks is __TRUSTED__
    /// because neither signature nor block hash contain this information.
    fn event_stream(
        self,
        start: BlockNumber,
        stop: BlockNumber,
        event_counts_stream: impl Stream<Item = anyhow::Result<usize>>,
    ) -> impl Stream<Item = Result<PeerData<EventsForBlockByTransaction>, PeerData<anyhow::Error>>>;
}

pub trait BlockClient {
    fn transactions_for_block(
        self,
        block: BlockNumber,
    ) -> impl Future<
        Output = Option<(
            PeerId,
            impl Stream<Item = anyhow::Result<(TransactionVariant, Receipt)>> + Send,
        )>,
    > + Send;

    fn state_diff_for_block(
        self,
        block: BlockNumber,
        state_diff_length: u64,
    ) -> impl Future<Output = Result<Option<(PeerId, StateUpdateData)>, IncorrectStateDiffCount>> + Send;

    fn class_definitions_for_block(
        self,
        block: BlockNumber,
        declared_classes_count: u64,
    ) -> impl Future<Output = Result<Option<(PeerId, Vec<ClassDefinition>)>, ClassDefinitionsError>> + Send;

    fn events_for_block(
        self,
        block: BlockNumber,
    ) -> impl Future<Output = Option<(PeerId, impl Stream<Item = (TransactionHash, Event)> + Send)>> + Send;
}
