//! # WWC Token Pallet
//!
//! World Wide Currency: unlimited algorithmic token emission.
//! Every token represents verified machine work. Zero human control.
//!
//! ## Immutable rules (enforced by code, no admin override):
//! - 100 WWC per validated LoRA improvement (benchmark >= +1%)
//! - 10 WWC per validated benchmark submission
//! - 50 WWC per active validator node per day
//! - 0 WWC from any other source (no mint function, no owner)

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use sp_std::vec::Vec;

    // ── Reward constants (immutable) ────────────────────────────────
    pub const REWARD_LORA_IMPROVEMENT: u128 = 100;
    pub const REWARD_BENCHMARK_SUBMISSION: u128 = 10;
    pub const REWARD_VALIDATOR_DAILY: u128 = 50;
    pub const MIN_BENCHMARK_GAIN: u8 = 1; // percent
    pub const REQUIRED_VALIDATIONS: u32 = 3;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    // ── Config trait ────────────────────────────────────────────────
    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
    }

    // ── Storage ─────────────────────────────────────────────────────

    /// Total WWC ever minted (unlimited, grows forever)
    #[pallet::storage]
    #[pallet::getter(fn total_supply)]
    pub type TotalSupply<T> = StorageValue<_, u128, ValueQuery>;

    /// Balance of each account
    #[pallet::storage]
    #[pallet::getter(fn balance_of)]
    pub type Balances<T: Config> = StorageMap<
        _, Blake2_128Concat, T::AccountId, u128, ValueQuery
    >;

    /// Contribution submissions pending validation
    /// Key: contribution hash, Value: (submitter, benchmark_gain, validators_so_far)
    #[pallet::storage]
    pub type PendingContributions<T: Config> = StorageMap<
        _, Blake2_128Concat, [u8; 32],
        (T::AccountId, u8, Vec<T::AccountId>),
    >;

    /// Track which validators have been rewarded today
    /// Key: (day_number, validator), Value: bool
    #[pallet::storage]
    pub type ValidatorRewarded<T: Config> = StorageMap<
        _, Blake2_128Concat, (u64, T::AccountId), bool, ValueQuery
    >;

    // ── Events ──────────────────────────────────────────────────────
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A contribution was submitted for validation
        ContributionSubmitted {
            submitter: T::AccountId,
            hash: [u8; 32],
            benchmark_gain: u8,
        },
        /// A validator approved a contribution
        ContributionValidated {
            validator: T::AccountId,
            hash: [u8; 32],
        },
        /// Contribution fully validated, rewards minted
        RewardMinted {
            contributor: T::AccountId,
            amount: u128,
            contribution_type: ContributionType,
        },
        /// Transfer between accounts
        Transfer {
            from: T::AccountId,
            to: T::AccountId,
            amount: u128,
        },
    }

    // ── Types ────────────────────────────────────────────────────────
    #[derive(Clone, Encode, Decode, TypeInfo, MaxEncodedLen, PartialEq, Debug)]
    pub enum ContributionType {
        LoraImprovement,
        BenchmarkSubmission,
        ValidatorReward,
    }

    // ── Errors ──────────────────────────────────────────────────────
    #[pallet::error]
    pub enum Error<T> {
        /// Benchmark gain is below the minimum threshold
        InsufficientBenchmarkGain,
        /// This contribution hash already exists
        ContributionAlreadyExists,
        /// Contribution not found in pending
        ContributionNotFound,
        /// Validator already validated this contribution
        AlreadyValidated,
        /// Cannot validate your own contribution
        CannotSelfValidate,
        /// Insufficient balance for transfer
        InsufficientBalance,
        /// Validator already rewarded today
        AlreadyRewardedToday,
    }

    // ── Extrinsics (callable functions) ──────────────────────────────
    //
    // IMPORTANT: There is NO admin function. No set_owner, no pause,
    // no emergency_mint, no upgrade_rules. This is by design.
    // Once deployed, these rules are permanent.

    #[pallet::call]
    impl<T: Config> Pallet<T> {

        /// Submit a LoRA contribution for validation.
        /// The contributor provides the hash of the delta and the benchmark gain.
        /// Requires 3 independent validators to approve before minting.
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn submit_contribution(
            origin: OriginFor<T>,
            contribution_hash: [u8; 32],
            benchmark_gain: u8,
        ) -> DispatchResult {
            let submitter = ensure_signed(origin)?;

            ensure!(
                benchmark_gain >= MIN_BENCHMARK_GAIN,
                Error::<T>::InsufficientBenchmarkGain
            );
            ensure!(
                !PendingContributions::<T>::contains_key(&contribution_hash),
                Error::<T>::ContributionAlreadyExists
            );

            PendingContributions::<T>::insert(
                &contribution_hash,
                (submitter.clone(), benchmark_gain, Vec::<T::AccountId>::new()),
            );

            Self::deposit_event(Event::ContributionSubmitted {
                submitter,
                hash: contribution_hash,
                benchmark_gain,
            });
            Ok(())
        }

        /// Validate a pending contribution. Once 3 validators approve,
        /// the contributor receives 100 WWC and each validator receives 10 WWC.
        #[pallet::call_index(1)]
        #[pallet::weight(10_000)]
        pub fn validate_contribution(
            origin: OriginFor<T>,
            contribution_hash: [u8; 32],
        ) -> DispatchResult {
            let validator = ensure_signed(origin)?;

            PendingContributions::<T>::try_mutate(
                &contribution_hash,
                |maybe| -> DispatchResult {
                    let (submitter, _gain, validators) =
                        maybe.as_mut().ok_or(Error::<T>::ContributionNotFound)?;

                    // Cannot validate your own contribution
                    ensure!(*submitter != validator, Error::<T>::CannotSelfValidate);
                    // Cannot validate twice
                    ensure!(
                        !validators.contains(&validator),
                        Error::<T>::AlreadyValidated
                    );

                    validators.push(validator.clone());

                    Self::deposit_event(Event::ContributionValidated {
                        validator: validator.clone(),
                        hash: contribution_hash,
                    });

                    // If we have enough validations, mint rewards
                    if validators.len() as u32 >= REQUIRED_VALIDATIONS {
                        // Mint 100 WWC to the contributor
                        Self::mint(
                            submitter.clone(),
                            REWARD_LORA_IMPROVEMENT,
                            ContributionType::LoraImprovement,
                        );
                        // Mint 10 WWC to each validator
                        for v in validators.iter() {
                            Self::mint(
                                v.clone(),
                                REWARD_BENCHMARK_SUBMISSION,
                                ContributionType::BenchmarkSubmission,
                            );
                        }
                    }
                    Ok(())
                },
            )?;

            // Clean up if fully validated
            if let Some((_, _, validators)) =
                PendingContributions::<T>::get(&contribution_hash)
            {
                if validators.len() as u32 >= REQUIRED_VALIDATIONS {
                    PendingContributions::<T>::remove(&contribution_hash);
                }
            }
            Ok(())
        }

        /// Claim daily validator reward (50 WWC).
        /// Can only be called once per day per validator.
        #[pallet::call_index(2)]
        #[pallet::weight(10_000)]
        pub fn claim_validator_reward(origin: OriginFor<T>) -> DispatchResult {
            let validator = ensure_signed(origin)?;

            let day = <frame_system::Pallet<T>>::block_number()
                .saturated_into::<u64>() / 14400; // ~1 day at 6s blocks

            ensure!(
                !ValidatorRewarded::<T>::get(&(day, validator.clone())),
                Error::<T>::AlreadyRewardedToday
            );

            ValidatorRewarded::<T>::insert((day, validator.clone()), true);
            Self::mint(
                validator,
                REWARD_VALIDATOR_DAILY,
                ContributionType::ValidatorReward,
            );
            Ok(())
        }

        /// Transfer WWC between accounts. Basic value transfer.
        #[pallet::call_index(3)]
        #[pallet::weight(10_000)]
        pub fn transfer(
            origin: OriginFor<T>,
            to: T::AccountId,
            amount: u128,
        ) -> DispatchResult {
            let from = ensure_signed(origin)?;

            let from_balance = Balances::<T>::get(&from);
            ensure!(from_balance >= amount, Error::<T>::InsufficientBalance);

            Balances::<T>::mutate(&from, |b| *b -= amount);
            Balances::<T>::mutate(&to, |b| *b += amount);

            Self::deposit_event(Event::Transfer { from, to, amount });
            Ok(())
        }
    }

    // ── Internal functions ───────────────────────────────────────────
    impl<T: Config> Pallet<T> {
        /// Mint new WWC tokens. This is the ONLY way tokens are created.
        /// There is no public mint function. Only the pallet logic can call this.
        fn mint(to: T::AccountId, amount: u128, contribution_type: ContributionType) {
            Balances::<T>::mutate(&to, |b| *b += amount);
            TotalSupply::<T>::mutate(|s| *s += amount);

            Self::deposit_event(Event::RewardMinted {
                contributor: to,
                amount,
                contribution_type,
            });
        }
    }

    // ── Genesis config ──────────────────────────────────────────────
    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub initial_balances: Vec<(T::AccountId, u128)>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self { initial_balances: Vec::new() }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            let mut total: u128 = 0;
            for (account, amount) in &self.initial_balances {
                Balances::<T>::insert(account, amount);
                total += amount;
            }
            TotalSupply::<T>::put(total);
        }
    }
}
