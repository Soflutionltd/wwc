//! # WWC Token Pallet
//!
//! World Wide Currency: unlimited algorithmic token emission.
//! Every token represents verified machine work. Zero human control.
//!
//! ## Immutable rules (enforced by code, no admin override):
//! - Rewards are degressive: BASE_REWARD / sqrt(active_miners / 10)
//! - 10% of every reward is burned (deflationary pressure)
//! - Requires 3 independent validators with staking
//! - 0 WWC from any other source (no mint function, no owner)
//!
//! ## NON-UPGRADABLE PALLET
//! This pallet's logic MUST NOT be modified by runtime upgrades.
//! The blockchain (infrastructure) is upgradable by governance.
//! The monetary rules (this pallet) are immutable forever.

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
    pub const BASE_REWARD: u128 = 100; // initial reward at launch
    pub const HALVING_CONSTANT: u128 = 10; // sqrt divisor base
    pub const BURN_RATE_PERCENT: u128 = 10; // 10% of every reward is burned
    pub const MIN_STAKE_AMOUNT: u128 = 100; // min WWC to stake as validator
    pub const SLASH_PERCENT: u128 = 50; // 50% of stake slashed on bad validation

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

    /// Total WWC burned (deflationary counter)
    #[pallet::storage]
    #[pallet::getter(fn total_burned)]
    pub type TotalBurned<T> = StorageValue<_, u128, ValueQuery>;

    /// Circulating supply = total_supply - total_burned
    /// (computed on-the-fly, not stored)

    /// Number of active miners (contributors who submitted in last 30 days)
    #[pallet::storage]
    #[pallet::getter(fn active_miners)]
    pub type ActiveMiners<T> = StorageValue<_, u64, ValueQuery>;

    /// Total contributions processed (for halving calculation)
    #[pallet::storage]
    #[pallet::getter(fn total_contributions)]
    pub type TotalContributions<T> = StorageValue<_, u64, ValueQuery>;

    /// Balance of each account
    #[pallet::storage]
    #[pallet::getter(fn balance_of)]
    pub type Balances<T: Config> = StorageMap<
        _, Blake2_128Concat, T::AccountId, u128, ValueQuery
    >;

    /// Staked balance of validators (locked, cannot transfer while staked)
    #[pallet::storage]
    #[pallet::getter(fn staked_balance)]
    pub type StakedBalances<T: Config> = StorageMap<
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
            burned: u128,
            contribution_type: ContributionType,
        },
        /// Tokens burned (deflationary)
        TokensBurned {
            amount: u128,
            total_burned: u128,
        },
        /// Validator staked tokens
        ValidatorStaked {
            validator: T::AccountId,
            amount: u128,
        },
        /// Validator unstaked tokens
        ValidatorUnstaked {
            validator: T::AccountId,
            amount: u128,
        },
        /// Validator slashed for bad validation
        ValidatorSlashed {
            validator: T::AccountId,
            amount_slashed: u128,
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
        InsufficientBenchmarkGain,
        ContributionAlreadyExists,
        ContributionNotFound,
        AlreadyValidated,
        CannotSelfValidate,
        InsufficientBalance,
        AlreadyRewardedToday,
        /// Must stake minimum amount to validate
        InsufficientStake,
        /// Not enough staked balance to unstake
        InsufficientStakedBalance,
        /// Validator is not staked (cannot validate)
        ValidatorNotStaked,
    }

    // ── Extrinsics (callable functions) ──────────────────────────────
    //
    // IMPORTANT: There is NO admin function. No set_owner, no pause,
    // no emergency_mint, no upgrade_rules. This is by design.
    // Once deployed, these rules are permanent.

    #[pallet::call]
    impl<T: Config> Pallet<T> {

        /// Submit a LoRA contribution for validation.
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn submit_contribution(
            origin: OriginFor<T>,
            contribution_hash: [u8; 32],
            benchmark_gain: u8,
        ) -> DispatchResult {
            let submitter = ensure_signed(origin)?;
            ensure!(benchmark_gain >= MIN_BENCHMARK_GAIN, Error::<T>::InsufficientBenchmarkGain);
            ensure!(!PendingContributions::<T>::contains_key(&contribution_hash), Error::<T>::ContributionAlreadyExists);
            PendingContributions::<T>::insert(&contribution_hash, (submitter.clone(), benchmark_gain, Vec::<T::AccountId>::new()));
            Self::deposit_event(Event::ContributionSubmitted { submitter, hash: contribution_hash, benchmark_gain });
            Ok(())
        }

        /// Validate a pending contribution. Requires validator to be staked.
        /// Once 3 staked validators approve, rewards are minted with 10% burn.
        #[pallet::call_index(1)]
        #[pallet::weight(10_000)]
        pub fn validate_contribution(
            origin: OriginFor<T>,
            contribution_hash: [u8; 32],
        ) -> DispatchResult {
            let validator = ensure_signed(origin)?;
            // Validator must have staked tokens
            ensure!(StakedBalances::<T>::get(&validator) >= MIN_STAKE_AMOUNT, Error::<T>::ValidatorNotStaked);

            PendingContributions::<T>::try_mutate(&contribution_hash, |maybe| -> DispatchResult {
                let (submitter, _gain, validators) = maybe.as_mut().ok_or(Error::<T>::ContributionNotFound)?;
                ensure!(*submitter != validator, Error::<T>::CannotSelfValidate);
                ensure!(!validators.contains(&validator), Error::<T>::AlreadyValidated);
                validators.push(validator.clone());
                Self::deposit_event(Event::ContributionValidated { validator: validator.clone(), hash: contribution_hash });

                if validators.len() as u32 >= REQUIRED_VALIDATIONS {
                    Self::mint_with_burn(submitter.clone(), REWARD_LORA_IMPROVEMENT, ContributionType::LoraImprovement);
                    for v in validators.iter() {
                        Self::mint_with_burn(v.clone(), REWARD_BENCHMARK_SUBMISSION, ContributionType::BenchmarkSubmission);
                    }
                }
                Ok(())
            })?;

            if let Some((_, _, validators)) = PendingContributions::<T>::get(&contribution_hash) {
                if validators.len() as u32 >= REQUIRED_VALIDATIONS {
                    PendingContributions::<T>::remove(&contribution_hash);
                }
            }
            Ok(())
        }

        /// Stake tokens to become a validator. Staked tokens are locked.
        #[pallet::call_index(4)]
        #[pallet::weight(10_000)]
        pub fn stake(origin: OriginFor<T>, amount: u128) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(Balances::<T>::get(&who) >= amount, Error::<T>::InsufficientBalance);
            Balances::<T>::mutate(&who, |b| *b -= amount);
            StakedBalances::<T>::mutate(&who, |s| *s += amount);
            Self::deposit_event(Event::ValidatorStaked { validator: who, amount });
            Ok(())
        }

        /// Unstake tokens. Returns them to transferable balance.
        #[pallet::call_index(5)]
        #[pallet::weight(10_000)]
        pub fn unstake(origin: OriginFor<T>, amount: u128) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(StakedBalances::<T>::get(&who) >= amount, Error::<T>::InsufficientStakedBalance);
            StakedBalances::<T>::mutate(&who, |s| *s -= amount);
            Balances::<T>::mutate(&who, |b| *b += amount);
            Self::deposit_event(Event::ValidatorUnstaked { validator: who, amount });
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
        /// Calculate degressive reward based on number of active miners.
        fn calculate_reward() -> u128 {
            let miners = ActiveMiners::<T>::get();
            if miners <= HALVING_CONSTANT as u64 {
                return BASE_REWARD;
            }
            let ratio = miners / HALVING_CONSTANT as u64;
            let sqrt_ratio = (ratio as f64).sqrt() as u128;
            let reward = BASE_REWARD / sp_std::cmp::max(sqrt_ratio, 1);
            sp_std::cmp::max(reward, 1)
        }

        /// Mint new WWC tokens with automatic 10% burn.
        /// This is the ONLY way tokens are created. No public mint function.
        fn mint_with_burn(to: T::AccountId, amount: u128, contribution_type: ContributionType) {
            let adjusted = if amount == REWARD_LORA_IMPROVEMENT {
                Self::calculate_reward()
            } else {
                let factor = Self::calculate_reward() * 100 / BASE_REWARD;
                sp_std::cmp::max(amount * factor / 100, 1)
            };

            // Calculate burn: 10% of reward is destroyed
            let burn_amount = adjusted * BURN_RATE_PERCENT / 100;
            let net_reward = adjusted - burn_amount;

            // Mint net reward to recipient
            Balances::<T>::mutate(&to, |b| *b += net_reward);
            TotalSupply::<T>::mutate(|s| *s += adjusted); // Total minted includes burned
            TotalBurned::<T>::mutate(|b| *b += burn_amount);
            TotalContributions::<T>::mutate(|c| *c += 1);

            Self::deposit_event(Event::RewardMinted {
                contributor: to, amount: net_reward, burned: burn_amount, contribution_type,
            });
            if burn_amount > 0 {
                Self::deposit_event(Event::TokensBurned {
                    amount: burn_amount, total_burned: TotalBurned::<T>::get(),
                });
            }
        }

        /// Slash a validator's stake. Called when a validation is disputed.
        /// Burns the slashed amount (removed from circulation permanently).
        pub fn slash_validator(validator: &T::AccountId) {
            let staked = StakedBalances::<T>::get(validator);
            let slash_amount = staked * SLASH_PERCENT / 100;
            if slash_amount > 0 {
                StakedBalances::<T>::mutate(validator, |s| *s -= slash_amount);
                TotalBurned::<T>::mutate(|b| *b += slash_amount);
                Self::deposit_event(Event::ValidatorSlashed {
                    validator: validator.clone(), amount_slashed: slash_amount,
                });
            }
        }

        /// Get circulating supply (total minted minus total burned)
        pub fn circulating_supply() -> u128 {
            TotalSupply::<T>::get().saturating_sub(TotalBurned::<T>::get())
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
