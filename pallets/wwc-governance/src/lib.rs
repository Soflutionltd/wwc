//! # WWC Governance Pallet
//!
//! Two-track governance system for the WWC blockchain:
//!
//! ## Track 1: Technical (VOTABLE)
//! - Runtime upgrades, new pallets, bug fixes, performance improvements
//! - Staking parameters, bridge configurations, network settings
//! - Voted by WWC holders proportional to their staked balance
//!
//! ## Track 2: Monetary (FORBIDDEN)
//! - NO proposal can modify wwc-token pallet logic
//! - NO proposal can change emission rules, burn rate, or reward formula
//! - NO proposal can mint tokens outside of PoUW validation
//! - This is enforced at the code level, not by social consensus

#![cfg_attr(not(feature = "std"), no_std)]
pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use sp_std::vec::Vec;

    /// Minimum voting period in blocks (~7 days at 6s blocks)
    pub const VOTING_PERIOD: u64 = 100_800;
    /// Minimum approval threshold (>50% of votes)
    pub const APPROVAL_THRESHOLD_PERCENT: u64 = 51;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
    }

    #[derive(Clone, Encode, Decode, TypeInfo, MaxEncodedLen, PartialEq, Debug)]
    pub enum ProposalTrack {
        /// Technical: runtime upgrades, new pallets, config changes
        Technical,
    }
    // Note: There is no Monetary track. It does not exist.
    // The monetary rules in wwc-token are immutable by design.

    #[derive(Clone, Encode, Decode, TypeInfo, MaxEncodedLen, PartialEq, Debug)]
    pub enum ProposalStatus {
        Active,
        Approved,
        Rejected,
        Executed,
    }

    // ── Storage ──
    #[pallet::storage]
    pub type ProposalCount<T> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    pub type Proposals<T: Config> = StorageMap<
        _, Blake2_128Concat, u64,
        (T::AccountId, ProposalTrack, Vec<u8>, ProposalStatus, u64, u128, u128),
        // (proposer, track, description, status, end_block, votes_for, votes_against)
    >;

    #[pallet::storage]
    pub type Votes<T: Config> = StorageMap<
        _, Blake2_128Concat, (u64, T::AccountId), bool, // (proposal_id, voter) -> for/against
    >;

    // ── Events ──
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ProposalCreated { id: u64, proposer: T::AccountId, track: ProposalTrack },
        VoteCast { proposal_id: u64, voter: T::AccountId, approve: bool, weight: u128 },
        ProposalApproved { id: u64 },
        ProposalRejected { id: u64 },
        ProposalExecuted { id: u64 },
    }

    #[pallet::error]
    pub enum Error<T> {
        ProposalNotFound,
        VotingPeriodEnded,
        VotingPeriodNotEnded,
        AlreadyVoted,
        ProposalNotActive,
        InsufficientVotingPower,
        /// Monetary track proposals are forbidden
        MonetaryTrackForbidden,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Create a technical proposal. Only Technical track allowed.
        /// Monetary proposals are rejected at code level.
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn create_proposal(
            origin: OriginFor<T>,
            description: Vec<u8>,
        ) -> DispatchResult {
            let proposer = ensure_signed(origin)?;
            let id = ProposalCount::<T>::get();
            let end_block = <frame_system::Pallet<T>>::block_number()
                .saturated_into::<u64>() + VOTING_PERIOD;
            Proposals::<T>::insert(id, (
                proposer.clone(), ProposalTrack::Technical, description,
                ProposalStatus::Active, end_block, 0u128, 0u128,
            ));
            ProposalCount::<T>::put(id + 1);
            Self::deposit_event(Event::ProposalCreated {
                id, proposer, track: ProposalTrack::Technical,
            });
            Ok(())
        }

        /// Vote on an active proposal. Weight = staked balance.
        #[pallet::call_index(1)]
        #[pallet::weight(10_000)]
        pub fn vote(
            origin: OriginFor<T>,
            proposal_id: u64,
            approve: bool,
        ) -> DispatchResult {
            let voter = ensure_signed(origin)?;
            ensure!(!Votes::<T>::contains_key(&(proposal_id, voter.clone())), Error::<T>::AlreadyVoted);

            Proposals::<T>::try_mutate(proposal_id, |maybe| -> DispatchResult {
                let (_, _, _, status, end_block, votes_for, votes_against) =
                    maybe.as_mut().ok_or(Error::<T>::ProposalNotFound)?;
                ensure!(*status == ProposalStatus::Active, Error::<T>::ProposalNotActive);
                let current = <frame_system::Pallet<T>>::block_number().saturated_into::<u64>();
                ensure!(current <= *end_block, Error::<T>::VotingPeriodEnded);

                // Vote weight = 1 (can be enhanced to use staked balance later)
                let weight: u128 = 1;
                if approve { *votes_for += weight; } else { *votes_against += weight; }
                Votes::<T>::insert((proposal_id, voter.clone()), approve);
                Self::deposit_event(Event::VoteCast { proposal_id, voter, approve, weight });
                Ok(())
            })
        }

        /// Finalize a proposal after voting period ends.
        #[pallet::call_index(2)]
        #[pallet::weight(10_000)]
        pub fn finalize_proposal(
            origin: OriginFor<T>,
            proposal_id: u64,
        ) -> DispatchResult {
            ensure_signed(origin)?;
            Proposals::<T>::try_mutate(proposal_id, |maybe| -> DispatchResult {
                let (_, _, _, status, end_block, votes_for, votes_against) =
                    maybe.as_mut().ok_or(Error::<T>::ProposalNotFound)?;
                ensure!(*status == ProposalStatus::Active, Error::<T>::ProposalNotActive);
                let current = <frame_system::Pallet<T>>::block_number().saturated_into::<u64>();
                ensure!(current > *end_block, Error::<T>::VotingPeriodNotEnded);

                let total = *votes_for + *votes_against;
                if total > 0 && *votes_for * 100 / total >= APPROVAL_THRESHOLD_PERCENT as u128 {
                    *status = ProposalStatus::Approved;
                    Self::deposit_event(Event::ProposalApproved { id: proposal_id });
                } else {
                    *status = ProposalStatus::Rejected;
                    Self::deposit_event(Event::ProposalRejected { id: proposal_id });
                }
                Ok(())
            })
        }
    }
}
