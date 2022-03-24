// Copyright 2019-2022 Parity Technologies (UK) Ltd.
// This file is part of subxt.
//
// subxt is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// subxt is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with subxt.  If not, see <http://www.gnu.org/licenses/>.

use codec::Encode;
use frame_metadata::{
    ExtrinsicMetadata,
    RuntimeMetadataLastVersion,
    StorageEntryType,
};
use scale_info::{
    form::PortableForm,
    Field,
    PortableRegistry,
    TypeDef,
    Variant,
};
use std::collections::{
    HashMap,
    HashSet,
};

/// Internal byte representation for various metadata types utilized for
/// generating deterministic hashes between different rust versions.
#[repr(u8)]
enum MetadataHashableIDs {
    Field,
    Variant,
    TypeDef,
    Type,
    Pallet,
    Extrinsic,
}

/// Hashing function utilized internally.
fn hash(bytes: &[u8]) -> [u8; 32] {
    sp_core::hashing::sha2_256(bytes)
}

/// Obtain the hash representation of a `scale_info::Field`.
fn get_field_hash(
    registry: &PortableRegistry,
    field: &Field<PortableForm>,
    visited_ids: &mut HashSet<u32>,
) -> [u8; 32] {
    let mut bytes = vec![MetadataHashableIDs::Field as u8];

    field.name().encode_to(&mut bytes);
    field.type_name().encode_to(&mut bytes);
    bytes.extend(get_type_hash(registry, field.ty().id(), visited_ids));

    hash(&bytes)
}

/// Obtain the hash representation of a `scale_info::Variant`.
fn get_variant_hash(
    registry: &PortableRegistry,
    var: &Variant<PortableForm>,
    visited_ids: &mut HashSet<u32>,
) -> [u8; 32] {
    let mut bytes = vec![MetadataHashableIDs::Variant as u8];

    var.name().encode_to(&mut bytes);
    for field in var.fields() {
        bytes.extend(get_field_hash(registry, field, visited_ids));
    }

    hash(&bytes)
}

/// Obtain the hash representation of a `scale_info::TypeDef`.
fn get_type_def_hash(
    registry: &PortableRegistry,
    ty_def: &TypeDef<PortableForm>,
    visited_ids: &mut HashSet<u32>,
) -> [u8; 32] {
    let mut bytes = vec![MetadataHashableIDs::TypeDef as u8];

    let data = match ty_def {
        TypeDef::Composite(composite) => {
            let mut bytes = Vec::new();
            for field in composite.fields() {
                bytes.extend(get_field_hash(registry, field, visited_ids));
            }
            bytes
        }
        TypeDef::Variant(variant) => {
            let mut bytes = Vec::new();
            // The type at path `node_template_runtime::Call` contains variants of the pallets
            // registered in order. Swapping the order between two pallets would result
            // in a different hash, but the functionality is still identical.
            // Sort by variant name to result in deterministic hashing.
            let mut variants: Vec<_> = variant.variants().iter().collect();
            variants.sort_by_key(|variant| variant.name());
            for var in variants {
                bytes.extend(get_variant_hash(registry, var, visited_ids));
            }
            bytes
        }
        TypeDef::Sequence(sequence) => {
            let mut bytes = Vec::new();
            bytes.extend(get_type_hash(
                registry,
                sequence.type_param().id(),
                visited_ids,
            ));
            bytes
        }
        TypeDef::Array(array) => {
            let mut bytes = Vec::new();
            array.len().encode_to(&mut bytes);
            bytes.extend(get_type_hash(
                registry,
                array.type_param().id(),
                visited_ids,
            ));
            bytes
        }
        TypeDef::Tuple(tuple) => {
            let mut bytes = Vec::new();
            for field in tuple.fields() {
                bytes.extend(get_type_hash(registry, field.id(), visited_ids));
            }
            bytes
        }
        TypeDef::Primitive(primitive) => {
            let mut bytes = Vec::new();
            primitive.encode_to(&mut bytes);
            bytes
        }
        TypeDef::Compact(compact) => {
            let mut bytes = Vec::new();
            bytes.extend(get_type_hash(
                registry,
                compact.type_param().id(),
                visited_ids,
            ));
            bytes
        }
        TypeDef::BitSequence(bitseq) => {
            let mut bytes = Vec::new();
            bytes.extend(get_type_hash(
                registry,
                bitseq.bit_order_type().id(),
                visited_ids,
            ));
            bytes.extend(get_type_hash(
                registry,
                bitseq.bit_store_type().id(),
                visited_ids,
            ));
            bytes
        }
    };
    bytes.extend(data);
    hash(&bytes)
}

/// Obtain the hash representation of a `scale_info::Type` identified by id.
fn get_type_hash(
    registry: &PortableRegistry,
    id: u32,
    visited_ids: &mut HashSet<u32>,
) -> [u8; 32] {
    let ty = registry.resolve(id).unwrap();

    let mut bytes = vec![MetadataHashableIDs::Type as u8];
    ty.path().segments().encode_to(&mut bytes);
    // Guard against recursive types
    if !visited_ids.insert(id) {
        return hash(&bytes)
    }

    let ty_def = ty.type_def();
    bytes.extend(get_type_def_hash(registry, ty_def, visited_ids));

    hash(&bytes)
}

/// Obtain the hash representation of a `frame_metadata::ExtrinsicMetadata`.
fn get_extrinsic_hash(
    registry: &PortableRegistry,
    extrinsic: &ExtrinsicMetadata<PortableForm>,
) -> [u8; 32] {
    let mut visited_ids = HashSet::<u32>::new();
    let mut bytes = vec![MetadataHashableIDs::Extrinsic as u8];

    bytes.extend(get_type_hash(registry, extrinsic.ty.id(), &mut visited_ids));
    bytes.push(extrinsic.version);
    for signed_extension in extrinsic.signed_extensions.iter() {
        signed_extension.identifier.encode_to(&mut bytes);
        bytes.extend(get_type_hash(
            registry,
            signed_extension.ty.id(),
            &mut visited_ids,
        ));
        bytes.extend(get_type_hash(
            registry,
            signed_extension.additional_signed.id(),
            &mut visited_ids,
        ));
    }

    hash(&bytes)
}

/// Obtain the hash representation of a `frame_metadata::PalletMetadata`.
pub fn get_pallet_hash(
    registry: &PortableRegistry,
    pallet: &frame_metadata::PalletMetadata<PortableForm>,
    cache: &mut MetadataHasherCache,
) -> [u8; 32] {
    let mut bytes = vec![MetadataHashableIDs::Pallet as u8];
    let mut visited_ids = HashSet::<u32>::new();

    if let Some(cached) = cache.pallets.get(&pallet.name) {
        return *cached
    }

    if let Some(ref calls) = pallet.calls {
        bytes.extend(get_type_hash(registry, calls.ty.id(), &mut visited_ids));
    }
    if let Some(ref event) = pallet.event {
        bytes.extend(get_type_hash(registry, event.ty.id(), &mut visited_ids));
    }
    for constant in pallet.constants.iter() {
        bytes.extend(constant.name.as_bytes());
        bytes.extend(&constant.value);
        bytes.extend(get_type_hash(registry, constant.ty.id(), &mut visited_ids));
    }
    if let Some(ref error) = pallet.error {
        bytes.extend(get_type_hash(registry, error.ty.id(), &mut visited_ids));
    }
    if let Some(ref storage) = pallet.storage {
        bytes.extend(storage.prefix.as_bytes());
        for entry in storage.entries.iter() {
            bytes.extend(entry.name.as_bytes());
            entry.modifier.encode_to(&mut bytes);
            match &entry.ty {
                StorageEntryType::Plain(ty) => {
                    bytes.extend(get_type_hash(registry, ty.id(), &mut visited_ids));
                }
                StorageEntryType::Map {
                    hashers,
                    key,
                    value,
                } => {
                    hashers.encode_to(&mut bytes);
                    bytes.extend(get_type_hash(registry, key.id(), &mut visited_ids));
                    bytes.extend(get_type_hash(registry, value.id(), &mut visited_ids));
                }
            }
            bytes.extend(&entry.default);
        }
    }

    let pallet_hash = hash(&bytes);
    cache.pallets.insert(pallet.name.clone(), pallet_hash);
    pallet_hash
}

/// Obtain the hash representation of a `frame_metadata::RuntimeMetadataLastVersion`.
pub fn get_metadata_hash(
    metadata: &RuntimeMetadataLastVersion,
    cache: &mut MetadataHasherCache,
) -> [u8; 32] {
    // Collect all pairs of (pallet name, pallet hash).
    let mut pallets: Vec<(String, [u8; 32])> = metadata
        .pallets
        .iter()
        .map(|pallet| {
            let name = pallet.name.clone();
            let hash = get_pallet_hash(&metadata.types, pallet, cache);
            (name, hash)
        })
        .collect();
    // Sort by pallet name to create a deterministic representation of the underlying metadata.
    pallets.sort_by_key(|key| key.1);

    // Note: pallet name is excluded from hashing.
    // Each pallet has a hash of 32 bytes, and the vector is extended with
    // extrinsic hash and metadata ty hash (2 * 32).
    let mut bytes = Vec::with_capacity(pallets.len() * 32 + 64);
    for (_, hash) in pallets.iter() {
        bytes.extend(hash)
    }

    bytes.extend(get_extrinsic_hash(&metadata.types, &metadata.extrinsic));

    let mut visited_ids = HashSet::<u32>::new();
    bytes.extend(get_type_hash(
        &metadata.types,
        metadata.ty.id(),
        &mut visited_ids,
    ));

    hash(&bytes)
}

/// Metadata hasher internal cache.
#[derive(Clone, Debug)]
pub struct MetadataHasherCache {
    /// Cache of the pallets obtained from `get_pallet_hash`.
    pub(crate) pallets: HashMap<String, [u8; 32]>,
}

impl MetadataHasherCache {
    /// Creates an empty `MetadataHasherCache`.
    pub fn new() -> Self {
        Self {
            pallets: HashMap::new(),
        }
    }
}

impl Default for MetadataHasherCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        get_metadata_hash,
        get_pallet_hash,
        MetadataHasherCache,
    };
    use codec::Decode;
    use frame_metadata::{
        RuntimeMetadata::V14,
        RuntimeMetadataLastVersion,
        RuntimeMetadataPrefixed,
    };
    use scale_info::form::PortableForm;
    use std::fs;

    /// Metadata obtained from https://github.com/substrate-developer-hub/substrate-node-template.git
    /// tag: polkadot-v0.9.17, branch origin/main.
    static METADATA_PATH: &'static str = "./test-assets/node_template.scale";

    /// Metadata obtained from https://github.com/substrate-developer-hub/substrate-node-template.git
    /// tag: polkadot-v0.9.17, branch origin/main, via moving `Balances` pallet order as last
    /// pallet in `construct_runtime` macro.
    static _METADATA_SWAP_PATH: &'static str = "./test-assets/node_template_swap.scale";

    /// Load metadata from a given file path.
    fn load_metadata(path: &str) -> RuntimeMetadataLastVersion {
        let bytes = fs::read(path).expect("Cannot read metadata");
        let meta: RuntimeMetadataPrefixed =
            Decode::decode(&mut &bytes[..]).expect("Cannot decode scale metadata");

        match meta.1 {
            V14(v14) => v14,
            _ => panic!("Unsupported metadata version {:?}", meta.1),
        }
    }

    /// Obtain a reference to the pallet identified by name in the metadata.
    fn get_pallet<'a>(
        metadata: &'a RuntimeMetadataLastVersion,
        name: &'static str,
    ) -> &'a frame_metadata::PalletMetadata<PortableForm> {
        metadata
            .pallets
            .iter()
            .find(|pallet| pallet.name == name)
            .expect("Pallet not found")
    }

    #[test]
    fn check_pallet_cache() {
        static PALLET_NAME: &'static str = "Balances";

        let metadata = load_metadata(METADATA_PATH);
        let mut cache = MetadataHasherCache::new();
        // Cache must be empty.
        assert_eq!(cache.pallets.len(), 0);

        let pallet = get_pallet(&metadata, "Balances");
        let hash = get_pallet_hash(&metadata.types, &pallet, &mut cache);
        let cached_hash = cache
            .pallets
            .get(PALLET_NAME)
            .expect("Pallet cache should be present");

        // Cache entry must exist after calculation.
        assert_eq!(&hash, cached_hash);
    }

    #[test]
    fn cache_deterministic_hash() {
        let metadata = load_metadata(METADATA_PATH);

        // Cache intermediate pallet hashes, utilizing a different cache each time.
        let cache_per_pallet: Vec<_> = metadata
            .pallets
            .iter()
            .map(|pallet| {
                let mut cache = MetadataHasherCache::new();
                (
                    pallet.name.clone(),
                    get_pallet_hash(&metadata.types, &pallet, &mut cache),
                )
            })
            .collect();

        // Utilizing the same cache for pallet hashes must not result in different results.
        let mut cache = MetadataHasherCache::new();
        let one_cache: Vec<_> = metadata
            .pallets
            .iter()
            .map(|pallet| {
                (
                    pallet.name.clone(),
                    get_pallet_hash(&metadata.types, &pallet, &mut cache),
                )
            })
            .collect();

        assert_eq!(cache_per_pallet, one_cache);
    }

    #[test]
    fn check_metadata_cache() {
        let metadata = load_metadata(METADATA_PATH);

        // Cache must be populated with pallet hashes.
        let mut cache = MetadataHasherCache::new();
        let _hash = get_metadata_hash(&metadata, &mut cache);

        // Compare cache with individual pallets
        for pallet in metadata.pallets.iter() {
            let mut inner_cache = MetadataHasherCache::new();

            // Compare a fresh iteration over pallet with cache value.
            let hash = get_pallet_hash(&metadata.types, pallet, &mut inner_cache);
            assert_eq!(cache.pallets.get(pallet.name.as_str()).unwrap(), &hash);

            // Recalling pallet should result in the same value, even if populated by metadata.
            let re_hash = get_pallet_hash(&metadata.types, pallet, &mut cache);
            assert_eq!(cache.pallets.get(pallet.name.as_str()).unwrap(), &re_hash);
        }
    }
}
