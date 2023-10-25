use crate::SubstrateError;
use avail_subxt::api::nomad_home as home;
use color_eyre::Result;
use ethers_core::types::Signature;
use nomad_core::{RawCommittedMessage, SignedUpdate, SignedUpdateWithMeta, Update, UpdateMeta};
use std::convert::TryInto;
use codec::Decode;
// use hex_literal::hex;
use subxt::ext::sp_runtime::traits::Header;
use subxt::{
    dynamic::Value, ext::scale_value::scale::TypeId, storage::DynamicStorageAddress, Config,
    OnlineClient,
};
use subxt::events::EventDetails;
use subxt::ext::sp_core::H256;
use subxt::rpc::ChainBlock;

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
}

#[tokio::test]
async fn test_event() {
    let client = subxt::OnlineClient::<avail_subxt::AvailConfig>::from_url("wss://kate.avail.tools:443/ws").await;

    match client {
        Ok(cl) => {
            println!("Client is created");

            // for i in 574466u32..574567u32 {
            //     println!("Testing for block: {}", i);


                let hash = cl
                    .rpc()
                    .block_hash(Some(574560u32.into()))
                    .await;


                match hash {

                    Ok(h) => {
                        println!("Hash is {:?}", h);

                       let block = cl.rpc().block(h).await;

                        match block {
                            Ok(bl) => {


                                println!("{:?}", bl);
                            }
                            Err(_) => {}
                        }

                        match h {
                            None => {}
                            Some(ha) => {
                                let dispatch_events_res_ = cl
                                    .events()
                                    .at(Some(ha))
                                    .await;



                                match dispatch_events_res_ {
                                    Ok(dispatch_events_res) => {

                                        // codec::Decode::decode

                                        println!("{:?}", dispatch_events_res);

                                        // let mut i = dispatch_events_res.iter();
                                        // let f = i.next().unwrap().unwrap();
                                        // // let as_f = f.as_event().unwrap().unwrap();
                                        // println!("{}", f.pallet_name());
                                        // println!("{}", f.pallet_index());
                                        // println!("{:?}", as_f);

                                        let ev: Result<Vec<_>, _> =
                                            dispatch_events_res.find::<home::events::Dispatch>()
                                            .into_iter()
                                            .collect();
                                        println!("Event {:?}", ev);
                                    }
                                    Err(e) => {
                                        println!("Error filtering events {}", e);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        println!("Error fetching hash {}", e)
                    }
                }
            // }
        }
        Err(e) => {
            println!("Error creating a client {}", e);
        }
    }
}
