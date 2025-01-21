//! Pallet for managing the on-demand configuration.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
use sp_runtime::SaturatedConversion;

pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
pub use benchmarking::BenchmarkHelper;

const LOG_TARGET: &str = "order-inherent";

pub trait OnReward<AccountId, Balance, R: RewardSize<Balance>> {
	fn reward(rewardee: AccountId);
}

pub trait RewardSize<Balance> {
	fn reward_size() -> Balance;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use crate::weights::WeightInfo;
	use codec::MaxEncodedLen;
	use frame_support::{
		traits::{
			fungible::{Inspect, Mutate},
			tokens::Balance,
		},
		weights::Weight,
		DefaultNoBound,
	};
	use sp_runtime::{traits::AtLeast32BitUnsigned, AccountId32, RuntimeAppPublic, Saturating};

	/// The module configuration trait.
	#[pallet::config]
	pub trait Config:
		frame_system::Config<AccountId = AccountId32> + pallet_aura::Config + pallet_session::Config
	{
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Relay chain balance type.
		type RelayChainBalance: Balance;

		/// The admin origin for managing the on-demand configuration.
		type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Block number type.
		type BlockNumber: Parameter
			+ Member
			+ Default
			+ MaybeSerializeDeserialize
			+ MaxEncodedLen
			+ AtLeast32BitUnsigned;

		/// Given that we want to keep this pallet as generic as possible, we don't assume the type
		/// of the threshold.
		///
		/// We are adding this for implementations that have some kind of threshold and want it to
		/// be stored within the runtime.
		///
		/// For example, this threshold could represent the total weight of all the ready
		/// transactions from the pool, or their total fees.
		///
		/// NOTE: If there isn't a threshold parameter, this can simply be set to `()`.
		type ThresholdParameter: Member
			+ Parameter
			+ Default
			+ MaybeSerializeDeserialize
			+ MaxEncodedLen;

		/// The currency for rewarding order placers.
		type Currency: Mutate<Self::ValidatorId>;

		/// Type which implements the logic for rewarding order placers.
		type OnReward: OnReward<
			Self::ValidatorId,
			<Self::Currency as Inspect<Self::ValidatorId>>::Balance,
			Self::RewardSize,
		>;

		type RewardSize: RewardSize<<Self::Currency as Inspect<Self::ValidatorId>>::Balance>;

		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: crate::BenchmarkHelper<Self::ThresholdParameter>;

		/// Weight Info
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// The authorities for the current on-demand slot.
	#[pallet::storage]
	#[pallet::getter(fn authorities)]
	pub type Authorities<T: Config> =
		StorageValue<_, BoundedVec<T::AuthorityId, T::MaxAuthorities>, ValueQuery>;

	/// Defines how often a new on-demand order is created, based on the number of slots.
	///
	/// This will limit the block production rate. However, if set to a low value, collators
	/// will struggle to coordinate effectively, leading to unnecessary multiple orders being
	/// placed.
	#[pallet::storage]
	#[pallet::getter(fn slot_width)]
	pub type SlotWidth<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// The threshold parameter stored in the runtime state.
	///
	/// This will determine whether an on-demand order should be placed by a collator.
	#[pallet::storage]
	#[pallet::getter(fn threshold_parameter)]
	pub type ThresholdParameter<T: Config> = StorageValue<_, T::ThresholdParameter, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Configuration of the coretime chain was set.
		SlotWidthSet { width: u32 },
		/// Threshold parameter set.
		ThresholdParameterSet { parameter: T::ThresholdParameter },
		/// We rewarded the order placer.
		OrderPlacerRewarded { order_placer: T::ValidatorId },
	}

	#[pallet::error]
	#[derive(PartialEq)]
	pub enum Error<T> {
		/// Invalid proof provided for system events key
		InvalidProof,
		/// Failed to read the relay chain proof.
		FailedProofReading,
		/// Failed to get the order placer account based on the authority id.
		FailedToGetOrderPlacerAccount,
		/// We failed to decode inherent data.
		FailedToDecodeInherentData,
	}

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Initial threshold parameter.
		pub threshold_parameter: T::ThresholdParameter,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			ThresholdParameter::<T>::set(self.threshold_parameter.clone());
		}
	}

	// NOTE: always place pallet-on-demand after the aura pallet.
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			let mut weight = Weight::zero();

			let (maybe_order_placer, _weight) = Self::order_placer();
			weight += _weight;

			let Some(order_placer) = maybe_order_placer else {
				return weight;
			};

			weight += T::DbWeight::get().reads(1);
			let Some(order_placer_acc) = pallet_session::KeyOwner::<T>::get((
				sp_application_crypto::key_types::AURA,
				order_placer.to_raw_vec(),
			)) else {
				return weight;
			};

			// NOTE: Game theoretically we don't have to check who created the order...
			// Only the supposed creator has the benefit to do so...

			// The block in which the order is placed and the block in which the order gets
			// finalized are not the same block..

			// There are two solutions:
			// 1. Rely purely on game theory
			// 2. Provide the relay parent in which the order was placed.

			// TODO: add weight from weights.rs
			T::OnReward::reward(order_placer_acc);

			weight
		}

		fn on_finalize(_: BlockNumberFor<T>) {
			// Update to the latest AuRa authorities.
			//
			// By updating the authorities on finalize we will always have the previous set
			// from the previous block used within this pallet.
			Authorities::<T>::put(pallet_aura::Authorities::<T>::get());
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the slot width for on-demand blocks.
		///
		/// - `origin`: Must be Root or pass `AdminOrigin`.
		/// - `width`: The slot width in relay chain blocks.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::set_slot_width())]
		pub fn set_slot_width(origin: OriginFor<T>, width: u32) -> DispatchResult {
			T::AdminOrigin::ensure_origin_or_root(origin)?;

			SlotWidth::<T>::set(width.clone());
			Self::deposit_event(Event::SlotWidthSet { width });

			Ok(())
		}

		/// Set the threshold parameter.
		///
		/// - `origin`: Must be Root or pass `AdminOrigin`.
		/// - `parameter`: The threshold parameter.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::set_threshold_parameter())]
		pub fn set_threshold_parameter(
			origin: OriginFor<T>,
			parameter: T::ThresholdParameter,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin_or_root(origin)?;

			ThresholdParameter::<T>::set(parameter.clone());
			Self::deposit_event(Event::ThresholdParameterSet { parameter });

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		fn order_placer() -> (Option<T::AuthorityId>, Weight) {
			let slot_width = SlotWidth::<T>::get();
			let para_height = frame_system::Pallet::<T>::block_number();
			let authorities = Authorities::<T>::get();

			let slot: u128 = (para_height >> slot_width).saturated_into();
			let indx = slot % authorities.len() as u128;

			let authority_id = authorities.get(indx as usize).cloned();
			let weight = Weight::zero().saturating_add(T::DbWeight::get().reads(3));

			(authority_id, weight)
		}
	}

	impl<T: Config>
		OnReward<T::ValidatorId, <T::Currency as Inspect<T::ValidatorId>>::Balance, Self> for Pallet<T>
	{
		fn reward(rewardee: T::ValidatorId) {
			let reward_size = T::RewardSize::reward_size();
			if T::Currency::mint_into(&rewardee, reward_size).is_err() {
				return;
			};
			Self::deposit_event(Event::OrderPlacerRewarded { order_placer: rewardee });
		}
	}

	impl<T: Config> RewardSize<<T::Currency as Inspect<T::ValidatorId>>::Balance> for Pallet<T> {
		fn reward_size() -> <T::Currency as Inspect<T::ValidatorId>>::Balance {
			<T::Currency as Inspect<T::ValidatorId>>::Balance::default()
				.saturating_add(1_000_000u32.saturated_into())
		}
	}
}
