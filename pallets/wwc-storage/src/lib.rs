//! # World Wide Currency Storage Pallet
//!
//! References LoRA deltas stored on IPFS.
//! The blockchain stores ONLY the IPFS content hash (CID) and metadata.
//! The actual files live on the IPFS network, hosted by node operators.
//!
//! ## Architecture:
//! - Blockchain (on-chain): CID hash, file size, owner, host list
//! - IPFS (off-chain): actual LoRA delta files (~50 Mo each)
//! - Each AURA node runs both a blockchain node AND an IPFS node
//! - Users see one app, two systems work invisibly underneath
//!
//! ## Flow:
//! 1. Contributor generates a LoRA delta locally
//! 2. Delta is published to IPFS, gets a unique CID
//! 3. Contributor calls register_file() with the CID
//! 4. Other nodes pin the file on IPFS (automatic via daemon)
//! 5. Nodes that host the file earn mining rewards

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use sp_std::vec::Vec;

    // ── Constants ────────────────────────────────────────────────────
    pub const MAX_CID_LENGTH: u32 = 64;         // IPFS CID v1 max length
    pub const MAX_FILE_SIZE: u64 = 100_000_000;  // 100 Mo max per delta
    pub const MIN_REPLICAS: u32 = 3;             // min nodes hosting each file

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
    }

    // ── Storage types ───────────────────────────────────────────────

    /// An IPFS-hosted file referenced on-chain
    #[derive(Clone, Encode, Decode, TypeInfo, MaxEncodedLen)]
    #[scale_info(skip_type_params(T))]
    pub struct IpfsFile<T: Config> {
        /// Who uploaded the file to IPFS
        pub owner: T::AccountId,
        /// File size in bytes
        pub size: u64,
        /// Block when registered
        pub registered_at: BlockNumberFor<T>,
        /// Number of nodes currently hosting
        pub replica_count: u32,
    }

    /// Registry: IPFS CID hash (32 bytes) -> file metadata
    #[pallet::storage]
    pub type Files<T: Config> = StorageMap<
        _, Blake2_128Concat, [u8; 32], IpfsFile<T>,
    >;

    /// IPFS CID string for each file hash (for human-readable lookups)
    #[pallet::storage]
    pub type CidMapping<T: Config> = StorageMap<
        _, Blake2_128Concat, [u8; 32], BoundedVec<u8, ConstU32<64>>,
    >;

    /// Which nodes are hosting which file
    #[pallet::storage]
    pub type FileHosts<T: Config> = StorageMap<
        _, Blake2_128Concat, [u8; 32], Vec<T::AccountId>, ValueQuery,
    >;

    /// Total files on the network
    #[pallet::storage]
    #[pallet::getter(fn total_files)]
    pub type TotalFiles<T> = StorageValue<_, u64, ValueQuery>;

    // ── Events ──────────────────────────────────────────────────────
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// New LoRA delta registered (IPFS CID stored on-chain)
        FileRegistered {
            cid_hash: [u8; 32],
            owner: T::AccountId,
            size: u64,
        },
        /// A node started hosting a file on IPFS
        HostAnnounced {
            cid_hash: [u8; 32],
            host: T::AccountId,
            replica_count: u32,
        },
        /// A node stopped hosting
        HostRemoved {
            cid_hash: [u8; 32],
            host: T::AccountId,
        },
    }

    // ── Errors ──────────────────────────────────────────────────────
    #[pallet::error]
    pub enum Error<T> {
        FileAlreadyExists,
        FileNotFound,
        FileTooLarge,
        AlreadyHosting,
        NotHosting,
        InvalidCid,
    }

    // ── Callable functions ──────────────────────────────────────────
    #[pallet::call]
    impl<T: Config> Pallet<T> {

        /// Register a LoRA delta on-chain after publishing to IPFS.
        /// Only the CID hash and metadata go on-chain.
        /// The file itself lives on IPFS.
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn register_file(
            origin: OriginFor<T>,
            cid_hash: [u8; 32],
            ipfs_cid: BoundedVec<u8, ConstU32<64>>,
            file_size: u64,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;

            ensure!(file_size <= MAX_FILE_SIZE, Error::<T>::FileTooLarge);
            ensure!(ipfs_cid.len() > 0, Error::<T>::InvalidCid);
            ensure!(
                !Files::<T>::contains_key(&cid_hash),
                Error::<T>::FileAlreadyExists
            );

            let now = <frame_system::Pallet<T>>::block_number();
            Files::<T>::insert(&cid_hash, IpfsFile {
                owner: owner.clone(),
                size: file_size,
                registered_at: now,
                replica_count: 1, // owner hosts it initially
            });
            CidMapping::<T>::insert(&cid_hash, ipfs_cid);
            TotalFiles::<T>::mutate(|n| *n += 1);

            // Owner is the first host
            FileHosts::<T>::mutate(&cid_hash, |hosts| {
                hosts.push(owner.clone());
            });

            Self::deposit_event(Event::FileRegistered {
                cid_hash,
                owner,
                size: file_size,
            });
            Ok(())
        }

        /// Announce that your node has pinned a file on IPFS.
        /// The daemon does this automatically when it downloads a new delta.
        #[pallet::call_index(1)]
        #[pallet::weight(10_000)]
        pub fn announce_hosting(
            origin: OriginFor<T>,
            cid_hash: [u8; 32],
        ) -> DispatchResult {
            let host = ensure_signed(origin)?;

            ensure!(
                Files::<T>::contains_key(&cid_hash),
                Error::<T>::FileNotFound
            );

            FileHosts::<T>::try_mutate(&cid_hash, |hosts| -> DispatchResult {
                ensure!(!hosts.contains(&host), Error::<T>::AlreadyHosting);
                hosts.push(host.clone());
                Ok(())
            })?;

            // Update replica count
            Files::<T>::mutate(&cid_hash, |maybe| {
                if let Some(file) = maybe {
                    file.replica_count += 1;
                }
            });

            let count = FileHosts::<T>::get(&cid_hash).len() as u32;
            Self::deposit_event(Event::HostAnnounced {
                cid_hash,
                host,
                replica_count: count,
            });
            Ok(())
        }
    }
}
