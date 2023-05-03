use crate::SubstrateError;
use color_eyre::Result;
use ethers_core::types::Signature;
use nomad_core::{RawCommittedMessage, SignedUpdate, SignedUpdateWithMeta, Update, UpdateMeta};
use std::convert::TryInto;
use std::time::Duration;
use async_std::task::sleep;
use subxt::ext::sp_runtime::traits::Header;
use subxt::{dynamic::Value, ext::scale_value::scale::TypeId, storage::DynamicStorageAddress, Config, OnlineClient, Error};
use avail_subxt::api::nomad_home as home;
use subxt::error::MetadataError::ErrorNotFound;
use subxt::error::RpcError;
use tracing::{error, info};
use nomad_core::NomadError::IoError;

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
        let mut retry_count = 0;
        loop {
            match self.rpc().header(None).await {
                Ok(header) => {
                    let u32_header: Result<u32, SubstrateError> = (*header.unwrap().number()).try_into();

                    return u32_header
                        .map_err(|_| SubstrateError::CustomError("Couldn't convert block number to u32".into()));
                }
                Err(err) => {
                    retry_count += 1;
                    if retry_count == 5 {
                        return Err(SubstrateError::from(err));
                    }
                }
            };
            info!("Retrying {} header", retry_count);
            sleep(Duration::from_millis(300)).await;
        }
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

        let opt_block_hash = self.get_block_hash(final_block_number.into()).await?;

        Ok(self.storage().fetch(address, Some(opt_block_hash)).await?)
    }

    /// Fetch ordered signed updates from the specific `block_number`
    pub async fn fetch_sorted_updates_for_block(
        &self,
        block_number: u32,
    ) -> Result<Vec<SignedUpdateWithMeta>, SubstrateError> {
        // Get hash for block number
        let hash = self.get_block_hash(block_number.into()).await?;

        // Get updates from block
        let update_events_res: Result<Vec<_>, _> = self
            .events()
            .at(Some(hash))
            .await?
            .find::<home::events::Update>() // TODO: remove dependency on avail metadata
            .into_iter()
            .collect();

        let update_events = update_events_res?;

        // TODO: sort events

        // Map update events into SignedUpdates with meta
        Ok(update_events
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

        let hash = self.get_block_hash(block_number.into()).await?;

        // Get dispatch events from block
        let dispatch_events_res: Result<Vec<_>, _> = self
            .events()
            .at(Some(hash))
            .await?
            .find::<home::events::Dispatch>() // TODO: remove dependency on avail metadata
            .into_iter()
            .collect();

        let dispatch_events = dispatch_events_res?;

        // TODO: sort events

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

    async fn get_block_hash(&self, block_number: u32) -> Result<<T as Config>::Hash, SubstrateError> {
        let mut retry_count = 0;
        loop {
            match self.rpc().block_hash(Some(block_number.into())).await {
                Ok(res) => {
                    return Ok(res.unwrap());
                }
                Err(err) => {
                    retry_count += 1;
                    if retry_count == 5 {
                        return Err(SubstrateError::from(err));
                    }
                }
            };
            info!("Retrying {} get_block_hash", retry_count);
            sleep(Duration::from_millis(300)).await;
        }
    }
}
