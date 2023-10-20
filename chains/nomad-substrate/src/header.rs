use core::marker::PhantomData;

use codec::{Decode, Encode};
use parity_util_mem::MallocSizeOf;
use serde::{Deserialize, Deserializer, Serialize};
use subxt::ext::{
    sp_core::H256,
    sp_runtime::{
        traits::{BlakeTwo256, Hash, Header as SPHeader},
        Digest as XtDigest, DigestItem as XtDigestItem,
    },
};

use avail_subxt::api::runtime_types::{
    sp_runtime::generic::digest::{Digest as ApiDigest, DigestItem as ApiDigestItem},
};
use crate::header::extension::{ApiHeader, HeaderExtensionEnum};
use crate::header::extension::v1::HeaderExtension;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
#[serde(rename_all = "camelCase")]
pub struct Header1 {
    pub parent_hash: H256,
    #[serde(deserialize_with = "number_from_hex")]
    #[codec(compact)]
    pub number: u32,
    pub state_root: H256,
    pub extrinsics_root: H256,
    pub digest: XtDigest,
    pub extension: HeaderExtensionEnum,
}

impl Header1 {
    pub fn data_root(&self) -> H256 {
        match &self.extension {
            HeaderExtensionEnum::V1(ext) => ext.commitment.data_root,
        }
    }
}

impl MallocSizeOf for Header1 {
    fn size_of(&self, ops: &mut parity_util_mem::MallocSizeOfOps) -> usize {
        self.parent_hash.size_of(ops)
            + self.number.size_of(ops)
            + self.state_root.size_of(ops)
            + self.extrinsics_root.size_of(ops)
            + self.digest.size_of(ops)
    }
}

impl SPHeader for Header1 {
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type Number = u32;

    fn new(
        number: Self::Number,
        extrinsics_root: Self::Hash,
        state_root: Self::Hash,
        parent_hash: Self::Hash,
        digest: XtDigest,
    ) -> Self {
        Self {
            parent_hash,
            number,
            state_root,
            extrinsics_root,
            digest,
            extension: HeaderExtensionEnum::V1(Default::default()),
        }
    }

    fn number(&self) -> &Self::Number { &self.number }

    fn set_number(&mut self, number: Self::Number) { self.number = number; }

    fn extrinsics_root(&self) -> &Self::Hash { &self.extrinsics_root }

    fn set_extrinsics_root(&mut self, root: Self::Hash) { self.extrinsics_root = root; }

    fn state_root(&self) -> &Self::Hash { &self.state_root }

    fn set_state_root(&mut self, root: Self::Hash) { self.state_root = root; }

    fn parent_hash(&self) -> &Self::Hash { &self.parent_hash }

    fn set_parent_hash(&mut self, hash: Self::Hash) { self.parent_hash = hash; }

    fn digest(&self) -> &XtDigest { &self.digest }

    fn digest_mut(&mut self) -> &mut XtDigest { &mut self.digest }

    fn hash(&self) -> Self::Hash { <Self::Hashing as Hash>::hash_of(self) }
}

fn number_from_hex<'de, D>(deserializer: D) -> Result<u32, D::Error>
    where
        D: Deserializer<'de>,
{
    let buf = String::deserialize(deserializer)?;
    let without_prefix = buf.trim_start_matches("0x");
    Ok(u32::from_str_radix(without_prefix, 16).unwrap())
}




pub mod extension {
    use avail_subxt::api::runtime_types;
    use crate::header::extension::v1::HeaderExtension;

    pub mod v1 {
        use super::runtime_types;

        #[derive(
        ::subxt::ext::codec::Decode,
        ::subxt::ext::codec::Encode,
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        serde::Deserialize,
        serde::Serialize,
        )]
        #[serde(rename_all = "camelCase")]
        pub struct HeaderExtension {
            pub commitment:
            runtime_types::da_primitives::kate_commitment::KateCommitment,
            pub app_lookup:
            runtime_types::da_primitives::asdr::data_lookup::DataLookup,
        }
    }

    #[derive(
    ::subxt::ext::codec::Decode,
    ::subxt::ext::codec::Encode,
    Clone,
    Debug,
    Eq,
    PartialEq,
    serde::Deserialize,
    serde::Serialize,
    )]
    pub enum HeaderExtensionEnum {
        #[codec(index = 0)]
        V1(HeaderExtension),
    }

    #[derive(
    ::subxt::ext::codec::Decode,
    ::subxt::ext::codec::Encode,
    Clone,
    Debug,
    Eq,
    PartialEq,
    )]
    pub struct ApiHeader<_0, _1> {
        pub parent_hash: ::subxt::ext::sp_core::H256,
        #[codec(compact)]
        pub number: _0,
        pub state_root: ::subxt::ext::sp_core::H256,
        pub extrinsics_root: ::subxt::ext::sp_core::H256,
        pub digest: runtime_types::sp_runtime::generic::digest::Digest,
        pub extension: HeaderExtensionEnum,
        #[codec(skip)]
        pub __subxt_unused_type_params: ::core::marker::PhantomData<_1>,
    }
}
