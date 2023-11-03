use crate::SubstrateError;
use avail_subxt::api::nomad_home as home;
use color_eyre::Result;
use ethers_core::types::{Signature, H256};
use nomad_core::{RawCommittedMessage, SignedUpdate, SignedUpdateWithMeta, Update, UpdateMeta};
use std::collections::HashMap;
use std::convert::TryInto;
use subxt::ext::sp_runtime::traits::Header;
use subxt::{
    dynamic::Value, ext::scale_value::scale::TypeId, storage::DynamicStorageAddress, Config,
    OnlineClient,
};

/// Nomad wrapper around `subxt::OnlineClient`
#[derive(Clone)]
pub struct NomadOnlineClient<T: Config> {
    client: OnlineClient<T>,
    timelag: Option<u8>,
}

impl<T: Config> std::ops::Deref for NomadOnlineClient<T> {
    type Target = OnlineClient<T>;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl<T: Config> NomadOnlineClient<T>
where
    <T as Config>::BlockNumber: TryInto<u32>,
{
    /// Instantiate a new NomadOnlineClient
    pub fn new(client: OnlineClient<T>, timelag: Option<u8>) -> Self {
        Self { client, timelag }
    }

    /// Get most recent block number
    pub async fn get_block_number(&self) -> Result<u32, SubstrateError> {
        let header = self.rpc().header(None).await?.unwrap();
        let u32_header = (*header.number()).try_into();

        u32_header
            .map_err(|_| SubstrateError::CustomError("Couldn't convert block number to u32".into()))
    }

    /// Fetch value from storage with built-in timelag
    pub async fn storage_fetch(
        &self,
        address: &DynamicStorageAddress<'_, Value>,
    ) -> Result<Option<Value<TypeId>>, SubstrateError> {
        let block_number = self.get_block_number().await?;
        let final_block_number = self
            .timelag
            .map_or(block_number, |lag| block_number - lag as u32);

        let opt_block_hash = self
            .rpc()
            .block_hash(Some(final_block_number.into()))
            .await?;

        Ok(self.storage().fetch(address, opt_block_hash).await?)
    }

    /// Fetch ordered signed updates from the specific `block_number`
    pub async fn fetch_sorted_updates_for_block(
        &self,
        block_number: u32,
    ) -> Result<Vec<SignedUpdateWithMeta>, SubstrateError> {
        // Get hash for block number
        let hash = self
            .rpc()
            .block_hash(Some(block_number.into()))
            .await?
            .unwrap();

        // Get updates from block
        let update_events_res: Result<Vec<_>, _> = self
            .events()
            .at(Some(hash))
            .await?
            .find::<home::events::Update>()
            .into_iter()
            .collect();

        let update_events = update_events_res?;

        // explicit sort all updates so that previous updates are linked prev -> new root
        // multiple update events in the same block should be rare or absent
        let sorted_update_events: Vec<home::events::Update> = sort_update_events(update_events);

        // Map update events into SignedUpdates with meta
        Ok(sorted_update_events
            .into_iter()
            .map(|ev| {
                let signature = Signature::try_from(ev.signature.as_ref())
                    .expect("chain accepted invalid signature");

                SignedUpdateWithMeta {
                    signed_update: SignedUpdate {
                        update: Update {
                            home_domain: ev.home_domain,
                            previous_root: ev.previous_root,
                            new_root: ev.new_root,
                        },
                        signature,
                    },
                    metadata: UpdateMeta {
                        block_number: block_number as u64,
                        timestamp: None,
                    },
                }
            })
            .collect())
    }

    /// Fetch ordered signed updates from the specific `block_number`
    pub async fn fetch_sorted_messages_for_block(
        &self,
        block_number: u32,
    ) -> Result<Vec<RawCommittedMessage>, SubstrateError> {
        // Get hash for block number
        let hash = self
            .rpc()
            .block_hash(Some(block_number.into()))
            .await?
            .unwrap();

        // Get dispatch events from block
        let dispatch_events_res: Result<Vec<_>, _> = self
            .events()
            .at(Some(hash))
            .await?
            .find::<home::events::Dispatch>() // TODO: remove dependency on avail metadata
            .into_iter()
            .collect();

        let mut dispatch_events = dispatch_events_res?;

        // sort events by the leaf of the index which is the order in which they were added to the trie
        dispatch_events.sort_by_key(|d| d.leaf_index);

        // Map dispatches into raw committed messages
        Ok(dispatch_events
            .into_iter()
            .map(|ev| RawCommittedMessage {
                leaf_index: ev.leaf_index,
                committed_root: ev.committed_root,
                message: ev.message,
            })
            .collect())
    }
}

/// sort_update_events sorts events based on the previous and new root. In most cases there will be
/// only one event per block.
fn sort_update_events(update_events: Vec<home::events::Update>) -> Vec<home::events::Update> {
    if update_events.is_empty() {
        return vec![];
    }

    if update_events.len() == 1 {
        return update_events;
    }

    let mut map_new_roots: HashMap<H256, home::events::Update> = update_events
        .iter()
        .map(|event| (event.new_root, event.clone()))
        .collect();
    let mut map_previous_roots: HashMap<H256, home::events::Update> = update_events
        .iter()
        .map(|event| (event.previous_root, event.clone()))
        .collect();

    let first_element = update_events
        .iter()
        .find(|event| !map_new_roots.contains_key(&event.previous_root))
        .expect("there must be first element");

    let mut sorted: Vec<home::events::Update> = Vec::with_capacity(update_events.len());
    sorted.push(first_element.clone());

    for _ in update_events {
        let next = sorted.last().unwrap();
        if let Some(previous) = map_previous_roots.get(&next.new_root) {
            sorted.push(previous.clone())
        }
    }

    return sorted;
}

#[test]
fn test_sorting_of_events() {
    let update_events: Vec<home::events::Update> = vec![
        home::events::Update {
            home_domain: 2000,
            previous_root: H256([5u8; 32]),
            new_root: H256([1u8; 32]),
            signature: vec![],
        },
        home::events::Update {
            home_domain: 2000,
            previous_root: H256([7u8; 32]),
            new_root: H256([5u8; 32]),
            signature: vec![],
        },
        home::events::Update {
            home_domain: 2000,
            previous_root: H256([1u8; 32]),
            new_root: H256([3u8; 32]),
            signature: vec![],
        },
    ];

    let sorted = sort_update_events(update_events);

    // assert_eq!(update_events.len(), sorted.len(), "length not equal");
    assert_eq!(H256([5u8; 32]), sorted[0].new_root, "wrong root position");
    assert_eq!(H256([1u8; 32]), sorted[1].new_root, "wrong root position");
    assert_eq!(H256([3u8; 32]), sorted[2].new_root, "wrong root position");

    let single_element_sorted = sort_update_events(vec![{
        home::events::Update {
            home_domain: 2000,
            previous_root: H256([5u8; 32]),
            new_root: H256([1u8; 32]),
            signature: vec![4u8],
        }
    }]);

    assert_eq!(1, single_element_sorted.len(), "must have one element");
    assert_eq!(2000, single_element_sorted[0].home_domain);
    assert_eq!(H256([5u8; 32]), single_element_sorted[0].previous_root);
    assert_eq!(H256([1u8; 32]), single_element_sorted[0].new_root);
    assert_eq!(1, single_element_sorted[0].signature.len());
    assert_eq!(4u8, single_element_sorted[0].signature[0]);

    let empty = sort_update_events(vec![]);
    assert_eq!(0, empty.len(), "must be empty");

    // println!("{:?}", sorted);
}
