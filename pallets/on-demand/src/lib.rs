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

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    use codec::Codec;
    use codec::MaxEncodedLen;
    use codec::{Decode, Encode};
    use cumulus_pallet_parachain_system::{
        relay_state_snapshot::Error as RelayError, RelayChainStateProof, RelaychainStateProvider,
    };
    use frame_support::{traits::tokens::Balance, DefaultNoBound};
    use frame_system::EventRecord;
    use order_primitives::{well_known_keys::EVENTS, OrderInherentData};
    use polkadot_primitives::Id as ParaId;
    use scale_info::TypeInfo;
    use sp_runtime::traits::{AtLeast32BitUnsigned, Convert};
    use sp_runtime::RuntimeAppPublic;
    use alloc::{vec::Vec, boxed::Box};

    /// The module configuration trait.
    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_aura::Config + pallet_session::Config {
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

        /// Type for getting the current relay chain state.
        type RelayChainStateProvider: RelaychainStateProvider;

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
        fn on_finalize(_: BlockNumberFor<T>) {
            // Update to the latest AuRa authorities.
            //
            // By updating the authorities on finalize we will always have the previous set
            // from the previous block used within this pallet.
            Authorities::<T>::put(pallet_aura::Authorities::<T>::get());
        }
    }

    #[pallet::inherent]
    impl<T: Config> ProvideInherent for Pallet<T> {
        type Call = Call<T>;
        type Error = MakeFatalError<()>;

        const INHERENT_IDENTIFIER: InherentIdentifier =
            order_primitives::ON_DEMAND_INHERENT_IDENTIFIER;

        fn create_inherent(data: &InherentData) -> Option<Self::Call> {
            let data: OrderInherentData = data
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
    enum RelayChainEvent<AccountId: Codec, Balance: Codec> {
        OnDemandAssignmentProvider(OnDemandEvent<AccountId, Balance>),
    }

    #[derive(Debug, Clone, Eq, PartialEq, Encode, Decode, TypeInfo, MaxEncodedLen)]
    enum OnDemandEvent<AccountId: Codec, Balance: Codec> {
        OnDemandOrderPlaced {
            para_id: ParaId,
            spot_price: Balance,
            ordered_by: AccountId,
        },
        SpotPriceSet {
            spot_price: Balance,
        },
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight((0, DispatchClass::Mandatory))]
        pub fn create_order(
            origin: OriginFor<T>,
            data: OrderInherentData,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;

            let current_state = T::RelayChainStateProvider::current_relay_chain_state();
            let relay_state_proof = RelayChainStateProof::new(
                data.para_id,
                current_state.state_root,
                data.relay_storage_proof,
            )
            .expect("Invalid relay chain state proof");

            let events =
                relay_state_proof
                    .read_entry::<Vec<
                        Box<
                            EventRecord<
                                RelayChainEvent<T::AccountId, T::RelayChainBalance>,
                                T::Hash,
                            >,
                        >,
                    >>(EVENTS, None)
                    .map_err(|e| match e {
                        RelayError::ReadEntry(_) => Error::InvalidProof,
                        _ => Error::<T>::FailedProofReading,
                    })?;

            let result: Vec<(T::RelayChainBalance, T::AccountId)> = events
                .into_iter()
                .filter_map(|item| match item.event {
                    RelayChainEvent::OnDemandAssignmentProvider(OnDemandEvent::<
                        T::AccountId,
                        T::RelayChainBalance,
                    >::OnDemandOrderPlaced {
                        para_id,
                        spot_price,
                        ordered_by,
                    }) if para_id == data.para_id => Some((spot_price, ordered_by)),
                    _ => None,
                })
                .collect();

            let Some(order_placer) = Self::order_placer() else {
                return Ok(().into());
            };
            let order_placer_acc = pallet_session::KeyOwner::<T>::get((
                sp_application_crypto::key_types::AURA,
                order_placer.to_raw_vec(),
            ))
            .ok_or(Error::<T>::FailedToGetOrderPlacerAccount)?;

            let Some(order) = result.into_iter().find(|(_, ordered_by)| {
                // In most implementations the validator id is same as account id.
                <T as pallet_session::Config>::ValidatorIdOf::convert(ordered_by.clone())
                    == Some(order_placer_acc.clone())
            }) else {
                // TODO: is there anything to do?
                return Ok(().into());
            };

            // TODO: reward the order placer.

            Ok(().into())
        }

        /// Set the slot width for on-demand blocks.
        ///
        /// - `origin`: Must be Root or pass `AdminOrigin`.
        /// - `width`: The slot width in relay chain blocks.
        #[pallet::call_index(1)]
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
        #[pallet::call_index(2)]
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
        fn order_placer() -> Option<T::AuthorityId> {
            let slot_width = SlotWidth::<T>::get();
            let para_height = frame_system::Pallet::<T>::block_number();
            let authorities = Authorities::<T>::get();

            let slot: u128 = (para_height >> slot_width).saturated_into();
            let indx = slot % authorities.len() as u128;

            authorities.get(indx as usize).cloned()
        }
    }
}
