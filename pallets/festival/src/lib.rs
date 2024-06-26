//** About **//
	// Information regarding the pallet
    // note_1: The reason the festival duration storages are separate is to facilitate block iteration during hooks.
    //TODO-0 ensure movies exist
    //TODO-1 extract private festivals form the create festival ext
    //TODO-2 check if sorting a list and calling dedup is faster than calling retain
    //TODO-3 check if still registered in stat tracker at the start of a festival
    //TODO-4 check if no duplicate movies in extrinsic param when adding movies
    //TODO-5 change NameStringLimit to Desc, currently BoundedDescString
    //TODO-6 FestivalHasEnded to emit a list of all festivals that ended in that block
    //TODO-7 bugged and emits an error
    //TODO-8 check if creator is still registered when activating a festival
    //TODO-9 handle error conditions
    


#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;



#[frame_support::pallet]
pub mod pallet {
    
    //** Config **//

        //* Imports *// 

            use frame_support::{
                dispatch::DispatchResultWithPostInfo,
                pallet_prelude::*,
                traits::{
                    Currency,
                    ReservableCurrency,
                    ExistenceRequirement::{
                        AllowDeath,
                        KeepAlive,
                    },
                },
                PalletId
            };
            use frame_system::pallet_prelude::*;
            use codec::{Decode, Encode, MaxEncodedLen};
            use sp_runtime::{RuntimeDebug, traits::{AccountIdConversion, AtLeast32BitUnsigned, CheckedAdd, CheckedSub, CheckedDiv, Saturating, One}};
            use scale_info::prelude::vec::Vec;
            use core::convert::TryInto;
            use frame_support::BoundedVec;
            use scale_info::TypeInfo;
            use sp_std::{collections::btree_map::BTreeMap,vec};
            use kine_movie;
            use kine_tags;

            // why does this need to be a crate?
            use crate::pallet::kine_tags::{
                CategoryId as CategoryId,
                TagId as TagId
            };


        //* Config *//
        
            #[pallet::pallet]
            pub struct Pallet<T>(_);

            #[pallet::config]
            pub trait Config: frame_system::Config 
            + kine_movie::Config + kine_tags::Config + kine_stat_tracker::Config {
                type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
                // type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;
            
                type FestivalId: Member + Parameter + AtLeast32BitUnsigned + Default + Copy + MaxEncodedLen;
                
                type MaxMoviesInFest: Get<u32>;
                type MaxOwnedFestivals: Get<u32>;
                type MinFesBlockDuration: Get<u32>;
                type MaxFestivalsPerBlock: Get<u32>;
                type MaxVotes: Get<u32>;
                
                type FestBlockSafetyMargin: Get<u32>;

                type PalletId: Get<PalletId>;
            }
      
      
      

    //** Types **//	
    
        //* Types *//
            
            type BalanceOf<T> = <<T as kine_stat_tracker::Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

        //* Constants *//
        //* Enums *//
            
            // Keeps track of the status of a festival.
            // AwaitingActivation -> The festival must be manually activated by the owner to become "Active".
            // AwaitingStartBlock -> The festival has been activated and is awaiting the start block to become "Active".
            // Active -> The festival is currently active and can be voted on.
            // Finished -> The festival has concluded.
            // FinishedNotEnoughVotes -> The festival has concluded, but without the minimum amount of votes to determine a winner.
            #[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
            pub enum FestivalStatus {
                AwaitingActivation,
                AwaitingStartBlock,
                Active,
                Finished,
                FinishedNotEnoughVotes,
            }
        
        //* Structs *//

            #[derive(Clone, Encode, Copy, Decode, Eq, PartialEq, RuntimeDebug, MaxEncodedLen, TypeInfo)]
            pub struct Festival<FestivalId, AccountId, BoundedNameString, BoundedDescString, FestivalStatus, BalanceOf, VoteList, CategoryTagList, MoviesInFest> {
                pub id: FestivalId,
                pub owner: AccountId,
                pub name: BoundedNameString,
                pub description: BoundedDescString,
                pub status: FestivalStatus,
                pub max_entry: BalanceOf,
                pub total_lockup: BalanceOf,
                pub vote_list: VoteList,
                pub categories_and_tags: CategoryTagList,
                pub internal_movies: MoviesInFest,
                pub external_movies: MoviesInFest,
            }

            #[derive(Clone, Encode, Copy, Decode, Eq, PartialEq, RuntimeDebug, MaxEncodedLen, TypeInfo)]
            pub struct BlockAssignment<BoundedFestivals> {
                pub to_start: BoundedFestivals,
                pub to_end: BoundedFestivals,
            }

            #[derive(Clone, Encode, Copy, Decode, Eq, PartialEq, RuntimeDebug, MaxEncodedLen, TypeInfo)]
            pub struct Vote<AccountId, MovieId, Balance> {
                pub voter: AccountId,
                pub vote_for: MovieId,
                pub amount: Balance,
            }

            #[derive(Clone, Encode, Copy, Decode, Eq, PartialEq, RuntimeDebug, MaxEncodedLen, TypeInfo)]
            pub struct WalletData<BoundedFestivals> {
                pub all_festivals: BoundedFestivals,
                pub awaiting_activation_festivals: BoundedFestivals,
                pub awaiting_start_festivals: BoundedFestivals,
                pub active_festivals: BoundedFestivals,
                pub finished_festivals: BoundedFestivals,
                pub won_festivals: BoundedFestivals,
            }




    //** Storage **//

        //* Festivals *//   

            #[pallet::storage]
            #[pallet::getter(fn next_festival_id)]
            pub(super) type NextFestivalId<T: Config> = 
                StorageValue<
                    _, 
                    T::FestivalId, 
                    ValueQuery
                >;
    
            #[pallet::storage]
            #[pallet::getter(fn get_festival)]
            pub type Festivals<T: Config> = 
                StorageMap<
                    _, 
                    Blake2_128Concat, T::FestivalId, 
                    Festival<
                        T::FestivalId, 
                        T::AccountId,
                        BoundedVec<u8, T::NameStringLimit>, //BoundedNameString
                        BoundedVec<u8, T::NameStringLimit>, //TODO-5
                        FestivalStatus,
                        BalanceOf<T>, //BalanceOf
                        BoundedVec<Vote<T::AccountId, BoundedVec<u8, T::LinkStringLimit>, BalanceOf<T>>, T::MaxVotes>, //VoteList
                        BoundedVec<(CategoryId<T>, TagId<T>), T::MaxTags>, //CategoryTagList
                        BoundedVec<BoundedVec<u8, T::LinkStringLimit>, T::MaxMoviesInFest>, //MoviesInFest
                    >,
                    OptionQuery
                >;


        //* Block Assignments *// 

            // Stores either the start/end of festivals. 
            // To be iterated during hooks.
            #[pallet::storage]
            #[pallet::getter(fn get_block_assignments)]
            pub(super) type BlockAssignments<T: Config> = 
                StorageMap<
                    _,
                    Blake2_128Concat, BlockNumberFor<T>,
                    BlockAssignment<BoundedVec<T::FestivalId, T::MaxFestivalsPerBlock>>,
                >;



        //* Wallet Data *// 

            // Information regarding a wallet's address
            #[pallet::storage]
            #[pallet::getter(fn get_wallet_festival_data)]
            pub(super) type WalletFestivalData<T: Config> = 
                StorageMap<
                    _,
                    Blake2_128Concat, T::AccountId,
                    WalletData<BoundedVec<T::FestivalId, T::MaxOwnedFestivals>>,
                >;




    //** Events **//

        #[pallet::event]
        #[pallet::generate_deposit(pub(super) fn deposit_event)]
        pub enum Event<T: Config> {
            FestivalCreated(T::AccountId, T::FestivalId),
            MovieAddedToFestival(T::FestivalId, BoundedVec<u8, T::LinkStringLimit>, T::AccountId),
            MoviesAddedToFestival(T::FestivalId, T::AccountId),
            VotedForMovieInFestival(T::FestivalId, BoundedVec<u8, T::LinkStringLimit>, T::AccountId),
            FestivalHasBegun(T::FestivalId),
            // FestivalHasEnded(T::FestivalId), //TODO-6
            FestivalHasEnded(T::FestivalId, BoundedVec<T::AccountId, T::MaxVotes>), 
            FestivalHasEndedUnsuccessfully(T::FestivalId),
            FestivalActivated(T::FestivalId, T::AccountId),
            FestivalTokensClaimed(T::AccountId, BalanceOf<T>),
        }



    //** Errors **//
        
        #[pallet::error]
        pub enum Error<T> {
            Overflow,
            Underflow,
            BadMetadata,
            InsufficientBalance,
            WalletStatsRegistryRequired,
            NotEnoughBalance,
            
            PastStartDate,
            FestivalPeriodTooShort,
            NoFestivalAdminAccess,
            NotEnoughMoviesInFestival,
            NotAwaitingActivation,

            MovieAlreadyInFestival,
            MovieNotInFestival,
            InvalidFestival,

            NonexistentFestival,
            NonexistentMovie,
            FestivalNotActive,
            FestivalNotAcceptingNewMovies,
            CannotVoteInOwnFestival,

            VoteMaxAmountCannotBeZero,
            VoteValueTooHigh,
            VoteValueCannotBeZero,

            InvalidBlockPeriod,
        }





    //** Hooks **//

        #[pallet::hooks]
        impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
            
            // fn on_initialize(_now: BlockNumberFor<T>) -> Weight {
            //     0
            // }

            fn on_finalize(now: BlockNumberFor<T>){
                Self::hook_deactivate_festival(now);
                Self::hook_activate_festival(now);
            }
        }



    //** Extrinsics **//

        #[pallet::call]
        impl<T: Config> Pallet<T> {

            // Create a new public festival.
            // #[pallet::call_index(n)]#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1,1))]
            // pub fn create_public_festival(
            //     origin: OriginFor<T>,
            //     bounded_name: BoundedVec<u8, T::NameStringLimit>,
            //     bounded_description: BoundedVec<u8, T::NameStringLimit>,
            //     max_entry: BalanceOf<T>,
            //     start_block: T::BlockNumber,
            //     end_block: T::BlockNumber,
            //     category_tag_list: BoundedVec<(CategoryId<T>, TagId<T>), T::MaxTags>,
            // ) -> DispatchResult {
                
            //     let who = ensure_signed(origin)?;
			// 	// ensure!(
			// 	// 	kine_stat_tracker::Pallet::<T>::is_wallet_registered(who.clone())?,
			// 	// 	Error::<T>::WalletStatsRegistryRequired,
			// 	// );
                
            //     // validate category and tag
            //     let category_type: kine_tags::CategoryType<T>
            //         = TryInto::try_into("Festival".as_bytes().to_vec())
            //         .map_err(|_|Error::<T>::BadMetadata)?;
            //     kine_tags::Pallet::<T>::do_validate_tag_data(
            //         category_type.clone(), 
            //         category_tag_list.clone()
            //     )?;

            //     // ensure the block periods are valid
            //     let safe_start_time = start_block
            //         .checked_sub(&T::BlockNumber::from(T::FestBlockSafetyMargin::get()))
            //         .ok_or(Error::<T>::InvalidBlockPeriod)?;
            //     ensure!(
            //         frame_system::Pallet::<T>::block_number() < safe_start_time, 
            //         Error::<T>::PastStartDate
            //     );
            //     ensure!(
            //         end_block-safe_start_time >= T::BlockNumber::from(T::FestBlockSafetyMargin::get()), 
            //         Error::<T>::FestivalPeriodTooShort
            //     );

            //     // create the festival & bind the owner & validated blocks to it
            //     let festival_id = Self::do_create_festival(
            //         who.clone(),
            //         bounded_name, bounded_description, max_entry,
            //         category_tag_list.clone(), FestivalStatus::New
            //     )?;
            //     Self::do_bind_owners_to_festival(who.clone(), festival_id)?;
            //     Self::do_bind_duration_to_festival(festival_id, start_block, end_block)?;

            //     // parse the festival_id into a BoundedVec<u8, T::ContentStringLimit>
            //     let encoded: Vec<u8> = festival_id.encode();
            //     let bounded_content_id: BoundedVec<u8, T::ContentStringLimit> = 
            //         TryInto::try_into(encoded).map_err(|_|Error::<T>::BadMetadata)?;

            //     // update tags with the encoded bounded_content_id
            //     kine_tags::Pallet::<T>::do_update_tag_data(
            //         category_type, 
            //         category_tag_list,
            //         bounded_content_id,
            //     )?;

            //     Self::deposit_event(Event::FestivalCreated(who.clone(), festival_id));
            //     Ok(())
            // }



            // Create a new private festival. It needs to be manually activated by the
            // owner when desired. Therefore, it does not have any block parameters, and
            // the festival is not bound to any blocks until the festival is activated.
            #[pallet::call_index(0)]#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1,1))]
            pub fn create_festival(
                origin: OriginFor<T>,
                bounded_name: BoundedVec<u8, T::NameStringLimit>,
                bounded_description: BoundedVec<u8, T::NameStringLimit>, 
                max_entry: BalanceOf<T>,
                internal_movie_ids: BoundedVec<BoundedVec<u8, T::LinkStringLimit>, T::MaxMoviesInFest>,
                external_movie_ids: BoundedVec<BoundedVec<u8, T::LinkStringLimit>, T::MaxMoviesInFest>,
                category_tag_list: BoundedVec<(CategoryId<T>, TagId<T>), T::MaxTags>,
            ) -> DispatchResultWithPostInfo {
                
                let who = ensure_signed(origin)?;
                ensure!(
                    max_entry > BalanceOf::<T>::from(0u32),
                    Error::<T>::VoteMaxAmountCannotBeZero,
                );

                //TODO-7
                // for movie_id in internal_movie_ids.clone() {
                //     ensure!(
                //         kine_movie::Pallet::<T>::do_does_internal_movie_exist(movie_id.clone())?,
                //         Error::<T>::NonexistentMovie,
                //     );
                // }

                // for movie_id in external_movie_ids.clone() {
                //     ensure!(
                //         kine_movie::Pallet::<T>::do_does_external_movie_exist(movie_id.clone())?,
                //         Error::<T>::NonexistentMovie,
                //     );
                // }

                // validate category and tag
                let category_type: kine_tags::CategoryType<T>
                    = TryInto::try_into("Festival".as_bytes().to_vec())
                    .map_err(|_|Error::<T>::BadMetadata)?;
                kine_tags::Pallet::<T>::do_validate_tag_data(
                    category_type.clone(), 
                    category_tag_list.clone()
                )?;

                // create the festival & bind the owner to it
                let festival_id = Self::do_create_festival(
                    who.clone(),
                    bounded_name, bounded_description, max_entry,
                    internal_movie_ids, external_movie_ids,
                    category_tag_list.clone(), FestivalStatus::AwaitingActivation
                )?;
                Self::do_bind_owners_to_festival(who.clone(), festival_id)?;
                
                // parse the festival_id into a BoundedVec<u8, T::ContentStringLimit>
                let encoded: Vec<u8> = festival_id.encode();
                let bounded_content_id: BoundedVec<u8, T::ContentStringLimit> = 
                    TryInto::try_into(encoded).map_err(|_|Error::<T>::BadMetadata)?;

                // update tags with the encoded bounded_content_id
                kine_tags::Pallet::<T>::do_update_tag_data(
                    category_type, 
                    category_tag_list,
                    bounded_content_id,
                )?;

                Self::deposit_event(Event::FestivalCreated(who.clone(), festival_id));
                Ok(().into())
            }


            // Activate a festival with status "AwaitingActivation" if you are its owner. Festivals
            // are considered private before their activation. After activating the festival, a new
            // start and end block is supplied, as when starting a regular festival. The start block
            // must be at least FestBlockSafetyMargin blocks away from the current block.
            #[pallet::call_index(1)]#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1,1))]
            pub fn activate_festival(
                origin: OriginFor<T>,
                festival_id: T::FestivalId,
                start_block: BlockNumberFor<T>,
                end_block: BlockNumberFor<T>,
            )-> DispatchResultWithPostInfo{
                
                let who = ensure_signed(origin)?;

                // mutate the festival from storage
                Festivals::<T>::try_mutate_exists( festival_id.clone(),|fes| -> DispatchResult{
                    let festival = fes.as_mut().ok_or(Error::<T>::BadMetadata)?;

                    // ensure the owner owns the festival 
                    ensure!(
                        festival.owner == who.clone(),
                        Error::<T>::NoFestivalAdminAccess
                    );
                    // ensure the festival has at least 2 movies
                    ensure!(
                        festival.internal_movies.len() > 1 || festival.external_movies.len() > 1,
                        Error::<T>::NotEnoughMoviesInFestival
                    );
                    // ensure the status is AwaitingActivation
                    ensure!(
                        festival.status == FestivalStatus::AwaitingActivation,
                        Error::<T>::NotAwaitingActivation
                    );

                    // let duration_blocks: BlockNumberFor<T> =
                    //     TryInto::try_into(0u32).map_err(|_| Error::<T>::BadMetadata).unwrap();

                    // ensure the block periods are valid
                    let safe_start_time = start_block
                        .checked_sub(&BlockNumberFor::<T>::from(T::FestBlockSafetyMargin::get()))
                        .ok_or(Error::<T>::InvalidBlockPeriod)?;
                    ensure!(
                        frame_system::Pallet::<T>::block_number() < safe_start_time, 
                        Error::<T>::PastStartDate
                    );
                    ensure!(
                        end_block-safe_start_time >= BlockNumberFor::<T>::from(T::MinFesBlockDuration::get()), 
                        Error::<T>::FestivalPeriodTooShort
                    );

                    // update the festival ownership status
                    WalletFestivalData::<T>::try_mutate_exists( who.clone(), |wal_data| -> DispatchResult{
                        let wallet_data = wal_data.as_mut().ok_or(Error::<T>::NonexistentFestival)?;
                        
                        //filter the movie from the awaiting activation list
                        wallet_data.awaiting_activation_festivals.retain(
                            |fes_id| 
                            fes_id != &festival_id.clone()
                                
                        );
                        wallet_data.awaiting_start_festivals.try_push(festival_id).unwrap();
                        
                        Ok(())
                    })?;

                    //bind the duration to the festival
                    Self::do_bind_duration_to_festival(festival_id, start_block, end_block)?;
                    festival.status = FestivalStatus::AwaitingStartBlock;

                    Self::deposit_event(Event::FestivalActivated(festival_id, who));
                    Ok(().into())
                })?;

                Ok(().into())
            }




            // Activate a festival with status "AwaitingActivation" if you are its owner. Festivals
            // are considered private before their activation. After activating ASAP, the festival starts right away,
            // so only an end block needs to be supplied.
            #[pallet::call_index(2)]#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1,1))]
            pub fn activate_festival_asap(
                origin: OriginFor<T>,
                festival_id: T::FestivalId,
                end_block: BlockNumberFor<T>,
            )-> DispatchResultWithPostInfo{
                
                let who = ensure_signed(origin)?;
                
                // mutate the festival from storage
                Festivals::<T>::try_mutate_exists( festival_id,|fes| -> DispatchResult{
                    let festival = fes.as_mut().ok_or(Error::<T>::BadMetadata)?;

                    // ensure the owner owns the festival 
                    ensure!(
                        festival.owner == who.clone(),
                        Error::<T>::NoFestivalAdminAccess
                    );
                    // ensure the festival has at least 2 movies
                    ensure!(
                        festival.internal_movies.len() > 1 || festival.external_movies.len() > 1,
                        Error::<T>::NotEnoughMoviesInFestival
                    );
                    // ensure the status is AwaitingActivation
                    ensure!(
                        festival.status == FestivalStatus::AwaitingActivation,
                        Error::<T>::NotAwaitingActivation
                    );

                    // ensure the block periods are valid
                    let now = frame_system::Pallet::<T>::block_number();
                    ensure!(
                        end_block - now >= BlockNumberFor::<T>::from(T::MinFesBlockDuration::get()), 
                        Error::<T>::FestivalPeriodTooShort
                    );

                    // update the festival ownership status
                    WalletFestivalData::<T>::try_mutate_exists( who.clone(), |wal_data| -> DispatchResult{
                        let wallet_data = wal_data.as_mut().ok_or(Error::<T>::NonexistentFestival)?;
                        
                        //filter the movie from the awaiting activation list
                        wallet_data.awaiting_activation_festivals.retain(
                            |fes_id| 
                            fes_id != &festival_id.clone()
                                
                        );
                        wallet_data.active_festivals.try_push(festival_id).unwrap();
                        
                        Ok(().into())
                    })?;

                    //bind the duration to the festival
                    festival.status = FestivalStatus::Active;

                    Self::deposit_event(Event::FestivalActivated(festival_id, who));
                    Ok(().into())
                })?;

                Ok(().into())
            }
            
            

            // Add a list of internal movies and a list of external movies to the festival.
            // Duplicate movies are filtered and only unique movies are inserted. 
            // Only works if the festival has not begun (i.e. its status is "New").           
            #[pallet::call_index(3)]#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1,1))]
            pub fn add_movies_to_fest(
                origin: OriginFor<T>,
                festival_id: T::FestivalId,
                mut internal_movie_ids: BoundedVec<BoundedVec<u8, T::LinkStringLimit>, T::MaxMoviesInFest>,
                mut external_movie_ids: BoundedVec<BoundedVec<u8, T::LinkStringLimit>, T::MaxMoviesInFest>,
            )-> DispatchResultWithPostInfo{
            
                let who = ensure_signed(origin)?;
                // ensure!(
				// 	kine_stat_tracker::Pallet::<T>::is_wallet_registered(who.clone())?,
				// 	Error::<T>::WalletStatsRegistryRequired,
				// );

                //TODO-7
                // for movie_id in internal_movie_ids.clone() {
                //     ensure!(
                //         kine_movie::Pallet::<T>::do_does_internal_movie_exist(movie_id.clone())?,
                //         Error::<T>::NonexistentMovie,
                //     );
                // }

                // for movie_id in external_movie_ids.clone() {
                //     ensure!(
                //         kine_movie::Pallet::<T>::do_does_external_movie_exist(movie_id.clone())?,
                //         Error::<T>::NonexistentMovie,
                //     );
                // }

                Festivals::<T>::try_mutate_exists(festival_id, |festival| -> DispatchResult {
                    let fes = festival.as_mut().ok_or(Error::<T>::NonexistentFestival)?;
                    ensure!(
                        fes.status == FestivalStatus::AwaitingActivation,
                        Error::<T>::FestivalNotAcceptingNewMovies
                    );

                    // filter out movies already in the festival
                    internal_movie_ids.retain(
                        |movie_id| 
                        !fes.internal_movies.contains(movie_id)
                    );
                    external_movie_ids.retain(
                        |movie_id| 
                        !fes.external_movies.contains(movie_id)
                    );

                    // add the movies to the festival
                    for internal_movie in internal_movie_ids {
                        fes.internal_movies.try_push(internal_movie);
                    }

                    // add the movies to the festival
                    for external_movie in external_movie_ids {
                        fes.external_movies.try_push(external_movie);
                    }
                    
                    Ok(().into())
                })?;

                Self::deposit_event(Event::MoviesAddedToFestival(festival_id, who.clone()));
                Ok(().into())
            }
            


            // Remove a list of internal movies and a list of external movies from the festival.
            // Only works if the festival has not begun (i.e. its status is "New").
            #[pallet::call_index(4)]#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1,1))]
            pub fn remove_movies_from_fest(
                origin: OriginFor<T>,
                festival_id: T::FestivalId,
                internal_movie_ids: BoundedVec<BoundedVec<u8, T::LinkStringLimit>, T::MaxMoviesInFest>,
                external_movie_ids: BoundedVec<BoundedVec<u8, T::LinkStringLimit>, T::MaxMoviesInFest>,
            )-> DispatchResultWithPostInfo{
            
                let who = ensure_signed(origin)?;
                // ensure!(
				// 	kine_stat_tracker::Pallet::<T>::is_wallet_registered(who.clone())?,
				// 	Error::<T>::WalletStatsRegistryRequired,
				// );
               
                Festivals::<T>::try_mutate_exists(festival_id, |festival| -> DispatchResult {
                    let fes = festival.as_mut().ok_or(Error::<T>::BadMetadata)?;
                    ensure!(
                        fes.status == FestivalStatus::AwaitingActivation,
                        Error::<T>::FestivalNotAcceptingNewMovies
                    );

                    //filter only the movies not in internal_movie_ids
                    fes.internal_movies.retain(
                        |movie_id| 
                        !internal_movie_ids.contains(movie_id)
                    );

                    //filter only the movies not in external_movie_ids
                    fes.external_movies.retain(
                        |movie_id| 
                        !external_movie_ids.contains(movie_id)
                    );
                    
                    Ok(().into())
                })?;

                Self::deposit_event(Event::MoviesAddedToFestival(festival_id, who.clone()));
                Ok(().into())
            }
        


            // Cast a vote for a movie included in the festival.
            #[pallet::call_index(5)]#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1,1))]
            pub fn vote_for_movie_in_festival(
                origin: OriginFor<T>,
                festival_id: T::FestivalId,
                movie_id : BoundedVec<u8, T::LinkStringLimit>,
                vote_amount: BalanceOf<T>,
            )-> DispatchResultWithPostInfo{
                
                let who = ensure_signed(origin)?;

                Self::do_vote_for_movie_in_festival(&who,festival_id, movie_id.clone(), vote_amount)?;

                Self::deposit_event(Event::VotedForMovieInFestival(festival_id, movie_id, who.clone()));
                Ok(().into())
            }
        


			#[pallet::call_index(6)]#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1,1))]
			pub fn claim_festival_rewards(
				origin: OriginFor<T>,
			) -> DispatchResultWithPostInfo {
				
				let who = ensure_signed(origin)?;
				
				let mut reward = BalanceOf::<T>::from(0u32);
				
				let claimable_tokens_festival = 
                    kine_stat_tracker::Pallet::<T>::
                    get_wallet_tokens(who.clone()).unwrap()
                    .claimable_tokens_festival;

                <T as kine_stat_tracker::Config>::Currency::transfer(
                    &Self::account_id(),  &who.clone(),
                    claimable_tokens_festival.clone(), AllowDeath, 
                );
                    
                kine_stat_tracker::Pallet::<T>::do_update_wallet_tokens(
                    who.clone(), 
                    kine_stat_tracker::FeatureType::Festival,
                    kine_stat_tracker::TokenType::Claimable,
                    BalanceOf::<T>::from(0u32), true
                )?;
			
				Self::deposit_event(Event::FestivalTokensClaimed(who, reward));
				Ok(().into())
			}	
            

        }


        
    //** Helpers **//

        impl<T: Config> Pallet<T> {

            //* Festival *//

                pub fn do_create_festival(
                    who: T::AccountId,
                    name: BoundedVec<u8, T::NameStringLimit>,
                    description: BoundedVec<u8, T::NameStringLimit>,
                    min_ticket_price: BalanceOf<T>,
                    internal_movie_ids: BoundedVec<BoundedVec<u8, T::LinkStringLimit>, T::MaxMoviesInFest>,
                    external_movie_ids: BoundedVec<BoundedVec<u8, T::LinkStringLimit>, T::MaxMoviesInFest>,
                    category_tag_list: BoundedVec<(CategoryId<T>, TagId<T>), T::MaxTags>,
                    status: FestivalStatus,
                ) -> Result<T::FestivalId, DispatchError> {

                    let festival_id =
                        NextFestivalId::<T>::try_mutate(|id| -> Result<T::FestivalId, DispatchError> {
                            let current_id = *id;
                            *id = id
                                .checked_add(&One::one())
                                .ok_or(Error::<T>::Overflow)?;
                            Ok(current_id)
                        })
                    ?;
            
                    let bounded_film_list: BoundedVec<BoundedVec<u8, T::LinkStringLimit>, T::MaxMoviesInFest>
                        = TryInto::try_into(Vec::new()).map_err(|_|Error::<T>::BadMetadata)?;
                    
                    let bounded_vote_list: BoundedVec<Vote<T::AccountId, BoundedVec<u8, T::LinkStringLimit>, BalanceOf<T>>, T::MaxVotes>
                        = TryInto::try_into(Vec::new()).map_err(|_|Error::<T>::BadMetadata)?;
                    
                    let zero_lockup = BalanceOf::<T>::from(0u32);
                    
                    let mut festival = Festival {
                        id: festival_id.clone(),
                        owner: who,
                        name: name,
                        description: description,
                        internal_movies: bounded_film_list.clone(),
                        external_movies: bounded_film_list,
                        status: status,
                        max_entry: min_ticket_price,
                        total_lockup: zero_lockup,
                        vote_list: bounded_vote_list,
                        categories_and_tags: category_tag_list,
                    };

                    // add the movies to the festival
                    for internal_movie in internal_movie_ids {
                        festival.internal_movies.try_push(internal_movie);
                    }

                    // add the movies to the festival
                    for external_movie in external_movie_ids {
                        festival.external_movies.try_push(external_movie);
                    }

                    Festivals::<T>::insert(festival_id, festival);
                    
                    Ok(festival_id)
                }


                pub fn do_bind_owners_to_festival(
                    who : T::AccountId,
                    festival_id : T::FestivalId,
                ) -> Result<(), DispatchError> {

                    if !WalletFestivalData::<T>::contains_key(who.clone()) {

                        let mut bounded_festival_list: BoundedVec<T::FestivalId, T::MaxOwnedFestivals>
                            = TryInto::try_into(Vec::new()).map_err(|_|Error::<T>::BadMetadata)?;
                        bounded_festival_list.try_push(festival_id).unwrap();
                        let bounded_empty_festival_list: BoundedVec<T::FestivalId, T::MaxOwnedFestivals>
                            = TryInto::try_into(Vec::new()).map_err(|_|Error::<T>::BadMetadata)?;

                        let new_data = WalletData {
                            all_festivals: bounded_festival_list.clone(),
                            awaiting_activation_festivals: bounded_festival_list,
                            awaiting_start_festivals: bounded_empty_festival_list.clone(),
                            active_festivals: bounded_empty_festival_list.clone(),
                            finished_festivals: bounded_empty_festival_list.clone(),
                            won_festivals: bounded_empty_festival_list,
                        };
                        WalletFestivalData::<T>::insert(who.clone(), new_data);

                    }
                    else {
                        WalletFestivalData::<T>::try_mutate( who.clone(), |festival_data| -> DispatchResult{
                            let fes_data = festival_data.as_mut().ok_or(Error::<T>::NonexistentFestival)?;
                            fes_data.all_festivals.try_push(festival_id).unwrap();
                            fes_data.awaiting_activation_festivals.try_push(festival_id).unwrap();
                            
                            Ok(())
                        })?;
                    }

                    Ok(())
                }


                pub fn do_bind_duration_to_festival(
                    festival_id : T::FestivalId,
                    start_block : BlockNumberFor<T>,
                    end_block: BlockNumberFor<T>
                ) -> Result<(), DispatchError> {
                    
                    // check if any entries exist for the start block and push the movie if true
                    if BlockAssignments::<T>::contains_key(start_block) {
                        BlockAssignments::<T>::mutate_exists(start_block, |assignments| -> DispatchResult {
                            let start_assignments = assignments.as_mut().ok_or(Error::<T>::BadMetadata)?;
                            
                            start_assignments.to_start.try_push(festival_id).unwrap();
                            Ok(())
                        })?;
                    }
                    // create a new entry for the start block if none exist and then push the movie
                    else {
                        let mut bounded_start_list: BoundedVec<T::FestivalId, T::MaxFestivalsPerBlock>
                            = TryInto::try_into(Vec::new()).map_err(|_|Error::<T>::BadMetadata)?;
                        bounded_start_list.try_push(festival_id).unwrap();
                        let mut bounded_end_list: BoundedVec<T::FestivalId, T::MaxFestivalsPerBlock>
                            = TryInto::try_into(Vec::new()).map_err(|_|Error::<T>::BadMetadata)?;
                        
                        let assignment = BlockAssignment {
                            to_start: bounded_start_list.clone(),
                            to_end: bounded_end_list.clone(),
                        };
                        BlockAssignments::<T>::insert(start_block.clone(), assignment);
                    }

                    
                    // check if any entries exist for the end block and push the movie if true
                    if BlockAssignments::<T>::contains_key(end_block) {
                        BlockAssignments::<T>::mutate_exists(end_block, |assignments| -> DispatchResult {
                            let end_block_assignments = assignments.as_mut().ok_or(Error::<T>::BadMetadata)?;
                            
                            end_block_assignments.to_end.try_push(festival_id).unwrap();
                            Ok(())
                        })?;
                    }
                    // create a new entry for the end block if none exist and then push the movie
                    else {
                        let mut bounded_start_list: BoundedVec<T::FestivalId, T::MaxFestivalsPerBlock>
                            = TryInto::try_into(Vec::new()).map_err(|_|Error::<T>::BadMetadata)?;
                        let mut bounded_end_list: BoundedVec<T::FestivalId, T::MaxFestivalsPerBlock>
                            = TryInto::try_into(Vec::new()).map_err(|_|Error::<T>::BadMetadata)?;
                        bounded_end_list.try_push(festival_id).unwrap();
                        
                        let assignment = BlockAssignment {
                            to_start: bounded_start_list.clone(),
                            to_end: bounded_end_list.clone(),
                        };
                        BlockAssignments::<T>::insert(end_block.clone(), assignment);
                    }

                    Ok(())
                }



                pub fn do_create_empty_block_assignments(
                    festival_id : T::FestivalId,
                ) -> Result<(), DispatchError> {

                    let mut bounded_start_list: BoundedVec<T::FestivalId, T::MaxOwnedFestivals>
                        = TryInto::try_into(Vec::new()).map_err(|_|Error::<T>::BadMetadata)?;
                    bounded_start_list.try_push(festival_id).unwrap();
                    
                    let mut bounded_end_list: BoundedVec<T::FestivalId, T::MaxOwnedFestivals>
                        = TryInto::try_into(Vec::new()).map_err(|_|Error::<T>::BadMetadata)?;
                    bounded_end_list.try_push(festival_id).unwrap();
                    
                    
                    let assignment = BlockAssignment {
                        to_start: bounded_start_list.clone(),
                        to_end: bounded_end_list.clone(),
                    };
                    
                    Ok(())
                }



            //* Hook Helpers *//

                fn hook_activate_festival(
                    now : BlockNumberFor<T>,
                ) -> DispatchResult {
                    
                    let fests = BlockAssignments::<T>::try_get(now);
                    ensure!(fests.is_ok(), Error::<T>::NonexistentFestival);

                    let festivals = fests.unwrap();
                    for festival_id in festivals.to_start.iter() {
                        Festivals::<T>::try_mutate_exists( festival_id,|festival| -> DispatchResult {
                            let fest = festival.as_mut().ok_or(Error::<T>::NonexistentFestival)?;

                            let is_fest_new = fest.status == FestivalStatus::AwaitingStartBlock;
                            // let is_creator_registered = (kine_stat_tracker::Pallet::<T>::is_wallet_registered(fest.owner.clone())?); //TODO-8
                            let is_creator_registered = true;
                            if is_fest_new && is_creator_registered {
                                // update the festival ownership status
                                WalletFestivalData::<T>::try_mutate_exists( fest.owner.clone(), |wal_data| -> DispatchResult{
                                    let wallet_data = wal_data.as_mut().ok_or(Error::<T>::NonexistentFestival)?;
                                    
                                    //filter the festival from the awaiting activation list
                                    wallet_data.awaiting_start_festivals.retain(
                                        |fes_id| 
                                        fes_id != &festival_id.clone()
                                    );
                                    wallet_data.active_festivals.try_push(festival_id.clone()).unwrap();
                                    
                                    Ok(())
                                })?;
                                
                                fest.status = FestivalStatus::Active;
                                Self::deposit_event(Event::FestivalHasBegun(festival_id.clone()));
                            } //TODO-9
                            
                            Ok(())
                        })?;
                    }
                    
                    Ok(())
                }


                fn hook_deactivate_festival(
                    now : BlockNumberFor<T>,
                ) -> DispatchResult {
                    
                    let fests = BlockAssignments::<T>::try_get(now);
                    ensure!(fests.is_ok(), Error::<T>::NonexistentFestival);
                    
                    let festivals = fests.unwrap();
                    for festival_id in festivals.to_end.iter() {
                        Festivals::<T>::try_mutate_exists( festival_id,|festival| -> DispatchResult{
                            let fest = festival.as_mut().ok_or(Error::<T>::NonexistentFestival)?;
                            
                            if fest.status == FestivalStatus::Active {
                                
                                // update the festival ownership status
                                Self::do_active_to_finished_fest_ownership(fest.owner.clone(), festival_id.clone());
                                
                                if fest.vote_list.len() > 1 {
                                    fest.status = FestivalStatus::Finished;
                                    let winners_list = Self::do_resolve_market(festival_id.clone())?;
                                    Self::deposit_event(Event::FestivalHasEnded(festival_id.clone(), winners_list));
                                }
                                else {
                                    fest.status = FestivalStatus::FinishedNotEnoughVotes;
                                    Self::deposit_event(Event::FestivalHasEndedUnsuccessfully(festival_id.clone()));
                                }

                            }

                            Ok(())
                        })?;
                    }
                    
                    Ok(())
                }


                // This function is isolated so that if it fails, the rest of the festivals
                // in the hook are not compromised.
                fn do_active_to_finished_fest_ownership(
                    owner: T::AccountId,
                    festival_id : T::FestivalId
                ) -> DispatchResult {
                    
                    // update the festival ownership status
                    WalletFestivalData::<T>::try_mutate_exists(owner, |wal_data| -> DispatchResult{
                        let wallet_data = wal_data.as_mut().ok_or(Error::<T>::NonexistentFestival)?;
                        
                        //filter the movie from the awaiting activation list
                        wallet_data.active_festivals.retain(
                            |fes_id| 
                            fes_id != &festival_id.clone()
                        );
                        wallet_data.finished_festivals.try_push(festival_id.clone()).unwrap();
                        
                        Ok(())
                    })?;

                    Ok(())
                }





            //** Movie **//

                pub fn do_vote_for_movie_in_festival(
                    who: &T::AccountId,
                    festival_id: T::FestivalId,
                    movie_id : BoundedVec<u8, T::LinkStringLimit>,
                    vote_amount : BalanceOf<T>,
                )-> Result<(), DispatchError> {
                    
                    Festivals::<T>::try_mutate_exists(festival_id, |festival| -> DispatchResult {
                        let fest = festival.as_mut().ok_or(Error::<T>::NonexistentFestival)?;   
                        
                        ensure!(
                            (fest.internal_movies.contains(&movie_id.clone())
                            || fest.external_movies.contains(&movie_id.clone())),
                            Error::<T>::MovieNotInFestival
                        );
                        ensure!(fest.owner != who.clone(), Error::<T>::CannotVoteInOwnFestival);
                        ensure!(fest.status == FestivalStatus::Active, Error::<T>::FestivalNotActive);
                        ensure!(vote_amount <= fest.max_entry, Error::<T>::VoteValueTooHigh);
                        ensure!(vote_amount >  BalanceOf::<T>::from(0u32), Error::<T>::VoteValueCannotBeZero);
                        
                        <T as kine_stat_tracker::Config>::Currency::transfer(
                            who, &Self::account_id(),
                            fest.max_entry, AllowDeath,
                        );
                        kine_stat_tracker::Pallet::<T>::do_update_wallet_tokens(
                            who.clone(), 
                            kine_stat_tracker::FeatureType::Festival,
                            kine_stat_tracker::TokenType::Locked,
                            vote_amount.clone(), false
                        ).unwrap();
                        
                        let vote = Vote {
                            voter: who.clone(),
                            vote_for: movie_id.clone(),
                            amount: vote_amount,
                        };

                        fest.total_lockup = fest.total_lockup.checked_add(&vote_amount).ok_or(Error::<T>::Overflow)?;
                        fest.vote_list.try_push(vote).unwrap();

                        Ok(())
                    })
                        
                }
            


            /* Treasury */

                fn account_id() -> T::AccountId {
                    <T as Config>::PalletId::get().try_into_account().unwrap()
                }


            /* Votes */

            fn do_resolve_market(
                festival_id: T::FestivalId
            ) -> Result<BoundedVec<T::AccountId, T::MaxVotes>, DispatchError> {
                
                let winning_opts = Self::do_get_winning_options(festival_id).unwrap();
                let winners_lockup = Self::do_get_winners_total_lockup(festival_id, winning_opts.clone()).unwrap();
                let mut winner_list : BoundedVec<T::AccountId, T::MaxVotes>
                    = TryInto::try_into(Vec::new()).map_err(|_|Error::<T>::BadMetadata)?;
                
                let festival = Festivals::<T>::try_get(festival_id).unwrap();
                // let total_lockup = festival.total_lockup;

                // let mut reward_exists = true;
                // if total_lockup == BalanceOf::<T>::from(0u32) {
                //     reward_exists = false;
                // }
                
                // determine the rewards (if > 0) and the voting winners
                for vote in festival.vote_list { 
                    if winning_opts.contains(&vote.vote_for.clone()) {
                        // if reward_exists {
                        let reward = Self::do_calculate_simple_reward(
                            festival.total_lockup, vote.amount.clone(), winners_lockup
                        ).unwrap();
                        kine_stat_tracker::Pallet::<T>::do_update_wallet_tokens(
                            vote.voter.clone(), 
                            kine_stat_tracker::FeatureType::Festival,
                            kine_stat_tracker::TokenType::Claimable,
                            reward, false,
                        ).unwrap();
                        // }
                        winner_list.try_push(vote.voter.clone()).unwrap();
                    }

                    // unlock the tokens from the votes
                    kine_stat_tracker::Pallet::<T>::do_update_wallet_tokens(
                        vote.voter.clone(), 
                        kine_stat_tracker::FeatureType::Festival,
                        kine_stat_tracker::TokenType::Locked,
                        vote.amount, true
                    ).unwrap();
                }

                Ok(winner_list)
            }


            fn do_get_winning_options(
                festival_id : T::FestivalId
            ) -> Result<Vec<BoundedVec<u8, T::LinkStringLimit>>,DispatchError> {
            
                let mut accumulator = BTreeMap::new();

                let fes_votes = Festivals::<T>::try_get(festival_id).unwrap().vote_list;
                for vote in fes_votes {
                    let movie_id = vote.vote_for;
                    let amount = vote.amount;
                    // amount -amount = 0 with Balance trait
                    let stat =  accumulator.entry(movie_id).or_insert(amount - amount);
                    *stat += amount;
                }

                let first_winner = accumulator
                    .iter()
                    .clone()
                    .max_by_key(|p| p.1)
                    .unwrap();
                
                let mut winners = vec![first_winner.0.clone()];
                
                // untie by adding all entries with the same lockup
                for (movie, lockup) in &accumulator {
                    if lockup == first_winner.1 && movie != first_winner.0 {
                        winners.push(movie.clone());
                    }
                }

                // verify if movies still exist, and assign the win to the uploader
                for movie_id in winners.clone() {
                    let internal_movie_exists = kine_movie::Pallet::<T>
                        ::do_does_internal_movie_exist(movie_id.clone())?;
                    let external_movie_exists = kine_movie::Pallet::<T>
                        ::do_does_external_movie_exist(movie_id.clone())?;

                    let uploader = kine_movie::Pallet::<T>
                        ::get_movie_uploader(movie_id)?;

                    // assign wins to the uploaders of the winning movies
                    if !WalletFestivalData::<T>::contains_key(uploader.clone()) {

                        let bounded_owned_list : BoundedVec<T::FestivalId, T::MaxOwnedFestivals>
                            = TryInto::try_into(Vec::new()).map_err(|_|Error::<T>::BadMetadata)?;
                        
                        let mut bounded_won_list : BoundedVec<T::FestivalId, T::MaxOwnedFestivals>
                            = TryInto::try_into(Vec::new()).map_err(|_|Error::<T>::BadMetadata)?;
                        bounded_won_list.try_push(festival_id).unwrap();

                        let new_data = WalletData {
                            all_festivals: bounded_owned_list.clone(),
                            awaiting_activation_festivals: bounded_owned_list.clone(),
                            awaiting_start_festivals: bounded_owned_list.clone(),
                            active_festivals: bounded_owned_list.clone(),
                            finished_festivals: bounded_owned_list,
                            won_festivals: bounded_won_list.clone(),
                        };
                        WalletFestivalData::<T>::insert(uploader.clone(), new_data);

                    }
                    else {
                        WalletFestivalData::<T>::try_mutate_exists( uploader.clone(), |festival_data| -> DispatchResult{
                            let fes_data = festival_data.as_mut().ok_or(Error::<T>::NonexistentFestival)?;
                            fes_data.won_festivals.try_push(festival_id).unwrap();
                            
                            Ok(())
                        })?;
                    }
                }
                
                Ok(winners)
            }


            fn do_get_winners_total_lockup(
                festival_id: T::FestivalId, 
                winning_movies:Vec<BoundedVec<u8, T::LinkStringLimit>>
            ) -> Result<BalanceOf<T>,DispatchError> {
                
                let fes_votes = Festivals::<T>::try_get(festival_id).unwrap().vote_list;
                let mut winners_total_lockup = BalanceOf::<T>::from(0u32);

                for vote in fes_votes {
                    if winning_movies.contains(&vote.vote_for.clone()) {
                        winners_total_lockup = 
                            winners_total_lockup
                            .checked_add(&vote.amount)
                            .ok_or(Error::<T>::Overflow)?;
                    }   
                }
            
                Ok(winners_total_lockup)
            }



            fn do_calculate_simple_reward(
                total_lockup: BalanceOf<T>,
                user_lockup: BalanceOf<T>,
                winner_lockup: BalanceOf<T>,
            ) -> Result<BalanceOf<T>, DispatchError> {
                let thousand: BalanceOf<T> = 1000u32.into();

                // let user_share = (user_lockup / winner_lockup);
                // user_lockup.saturating_mul(total_moderators.into());

                let user_share = 
                    winner_lockup
                    .checked_div(&user_lockup)
                    .ok_or(Error::<T>::Overflow)?;
                
                let user_reward = 
                    total_lockup
                    .checked_div(&user_share)
                    .ok_or(Error::<T>::Overflow)?;

                Ok(user_reward)
            }





        }
}
    