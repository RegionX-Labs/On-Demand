//! Pallet for managing the on-demand configuration.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame::{prelude::*, traits::Currency};
use polkadot_runtime_parachains::on_demand;

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

type BalanceOf<T> = <<T as on_demand::Config>::Currency as Currency<
	<T as frame_system::Config>::AccountId,
>>::Balance;
type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

#[frame::pallet]
pub mod pallet {
	use super::*;
	use crate::{frame_system::EventRecord, weights::WeightInfo};
	use cumulus_pallet_parachain_system::{
		relay_state_snapshot::Error as RelayError, RelayChainStateProof, RelaychainStateProvider,
	};
	use on_demand_primitives::{well_known_keys::EVENTS, OrderInherentData};
	use sp_runtime::traits::AtLeast32BitUnsigned;

	/// The module configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

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

		/// Type for getting the current relay chain state.
		type RelayChainStateProvider: RelaychainStateProvider;

		/// Weight Info
		type WeightInfo: WeightInfo;

		/// Relay chain on demand config.
		type OnDemandConfig: polkadot_runtime_parachains::on_demand::Config + Parameter + Member;

		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: crate::BenchmarkHelper<Self::ThresholdParameter>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Order sequence number.
	///
	/// Gets increased every time
	#[pallet::storage]
	#[pallet::getter(fn slot_width)]
	pub type SlotWidth<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

	/// Defines how often a new on-demand order is created, based on the number of slots.
	///
	/// This will limit the block production rate. However, if set to a low value, collators
	/// will struggle to coordinate effectively, leading to unnecessary multiple orders being
	/// placed.
	#[pallet::storage]
	#[pallet::getter(fn slot_width)]
	pub type SlotWidth<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

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
		SlotWidthSet { width: T::BlockNumber },
		/// Threshold parameter set.
		ThresholdParameterSet { parameter: T::ThresholdParameter },
	}

	#[pallet::error]
	#[derive(PartialEq)]
	pub enum Error<T> {
		/// Invalid proof provided for system events key
		InvalidProof,
		/// Failed to read the relay chain proof.
		FailedProofReading,
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

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = MakeFatalError<()>;

		const INHERENT_IDENTIFIER: InherentIdentifier =
			on_demand_primitives::ON_DEMAND_INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			let data: OrderInherentData<AccountIdOf<T::OnDemandConfig>> = data
				.get_data(&Self::INHERENT_IDENTIFIER)
				.ok()
				.flatten()
				.expect("there is not data to be posted; qed");

			Some(Call::create_order { data })
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::create_order { .. })
		}
	}

	#[derive(Debug, Clone, Eq, PartialEq, Encode, Decode, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	enum RelayChainEvent<T: polkadot_runtime_parachains::on_demand::Config> {
		OnDemandAssignmentProvider(on_demand::Event<T>),
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight((0, DispatchClass::Mandatory))]
		pub fn create_order(
			origin: OriginFor<T>,
			data: OrderInherentData<AccountIdOf<T::OnDemandConfig>>,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let current_state = T::RelayChainStateProvider::current_relay_chain_state();
			let relay_state_proof = RelayChainStateProof::new(
				data.para_id,
				current_state.state_root,
				data.relay_storage_proof,
			)
			.expect("Invalid relay chain state proof"); // TODO

			let events = relay_state_proof
				.read_entry::<Vec<Box<EventRecord<RelayChainEvent<T::OnDemandConfig>, T::Hash>>>>(
					EVENTS, None,
				)
				.map_err(|e| match e {
					RelayError::ReadEntry(_) => Error::InvalidProof,
					_ => Error::<T>::FailedProofReading,
				})
				.unwrap(); // TODO

			let result: Vec<(BalanceOf<T::OnDemandConfig>, AccountIdOf<T::OnDemandConfig>)> =
				events
					.into_iter()
					.filter_map(|item| match item.event {
						RelayChainEvent::<T::OnDemandConfig>::OnDemandAssignmentProvider(
							on_demand::Event::OnDemandOrderPlaced {
								para_id,
								spot_price,
								ordered_by,
							},
						) if para_id == data.para_id && ordered_by == data.author_pub =>
							Some((spot_price, ordered_by)),
						_ => None,
					})
					.collect();

			// TODO: Actually the order creator and the author will unlikely be the same account....
			// If we track slots we should be able to determine who was supposed to place an order.
			// 
			// TODO: simplest to have modified aura where the order creator is also the block author?
			// Keep in mind this wouldn't stall conesnsus...
			// Simply if the order creator doesn't create a block, a next order will be created and the
			// next collator will create a block.
			//
			// This of course ties consensus closely, however, for an initial version it should be good.
			//
			// NOTE: with this on-demand aura wouldn't make sense as it would be possible to stall
			// the chain... So we only want the slot based solution..

			// Since we filtered only the orders created by the author, we can simply take the first
			// one. There's no reason for the author to create multiple orders anyway.
			let order = result.first().unwrap(); // TODO

			Ok(().into())
		}

		/// Set the slot width for on-demand blocks.
		///
		/// - `origin`: Must be Root or pass `AdminOrigin`.
		/// - `width`: The slot width in relay chain blocks.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::set_slot_width())]
		pub fn set_slot_width(origin: OriginFor<T>, width: T::BlockNumber) -> DispatchResult {
			T::AdminOrigin::ensure_origin_or_root(origin)?;

			SlotWidth::<T>::set(width.clone());
			Self::deposit_event(Event::SlotWidthSet { width });

			Ok(())
		}

		/// Set the threshold parameter.
		///
		/// - `origin`: Must be Root or pass `AdminOrigin`.
		/// - `parameter`: The threshold parameter.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::set_threshold_parameter())]
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
}
