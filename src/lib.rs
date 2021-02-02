#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::sp_runtime::{traits::AccountIdConversion, ModuleId};
use frame_support::traits::Imbalance;
use frame_support::traits::{Currency, ExistenceRequirement, ReservableCurrency};
use frame_support::{
    decl_error, decl_event, decl_module, decl_storage, dispatch, ensure, traits::Get,
};
use frame_system::ensure_signed;
use glass_pumpkin::prime;
use num_bigint::BigUint;

/// Module Id of our pallet
/// It's used to get the pallet's treasury pool
const PALLET_ID: ModuleId = ModuleId(*b"Treasury");

/// Make the trait of our pallet
pub trait Trait: frame_system::Trait {
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
    type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;
}

/// Balance on our pallet
type BalanceOf<T> =
    <<T as Trait>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::Balance;

/// A Number, it is the number to find a solution for
pub type Number = u128;

/// A problem, it is stored on chain
#[derive(Debug, Encode, Decode)]
pub struct Problem<T: Trait> {
    /// The number to resolve
    number: Number,
    /// Amount of reward
    reward: BalanceOf<T>,
    /// The solution to the problem (when resolved)
    solution: Option<(u128, u128)>,
    /// Account id of the submitter
    submitter: T::AccountId,
    /// Account id of the resolver (when resolved)
    resolver: Option<T::AccountId>,
}

decl_storage! {
    trait Store for Module<T: Trait> as MangataModule {
        /// Map of all the problems submitted
        ProblemsMap: map hasher(identity) Number => Option<Problem<T>> = None;
    }
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as frame_system::Trait>::AccountId,
        Balance = BalanceOf<T>,
    {
        /// A problem got submitted
        /// [number, reward, who]
        ProblemSubmited(u128, Balance, AccountId),
        /// A problem was resolved
        /// [number, a, b, who]
        ProblemResolved(u128, u128, u128, AccountId),
    }
);

// Errors inform users that something went wrong.
decl_error! {
    pub enum Error for Module<T: Trait> {
        /// Trying to resolve a problem that doesn't exist
        InexistentNumber,
        /// Wrong answer to a problem
        WrongAnswer,
        /// Problem was already resolved
        AlreadyResolved,
        /// Problem was already submitted
        AlreadySubmitted,
    }
}

/// Return true when it's a prime number
fn is_prime(number: u128) -> bool {
    let number: BigUint = number.into();

    prime::check(&number)
}

impl<T: Trait> Module<T> {
    /// The account ID that holds the pallet's treasury pool
    fn account_id() -> T::AccountId {
        PALLET_ID.into_account()
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        // Errors must be initialized if they are used by the pallet.
        type Error = Error<T>;

        // Events must be initialized if they are used by the pallet.
        fn deposit_event() = default;

        /// Submit a problem
        /// 1 read & 3 writes to the db
        #[weight = T::DbWeight::get().reads_writes(1, 3) + 10_000]
        pub fn submit_problem(origin, number: u128, reward: BalanceOf<T>) -> dispatch::DispatchResult {
            let who = ensure_signed(origin)?;

            // Check if the problem was already submitted
            ensure!(!ProblemsMap::<T>::contains_key(number), Error::<T>::AlreadySubmitted);

            // Reserve the reward amount on the account
            T::Currency::reserve(&who, reward)?;

            // Insert new problem to resolve
            ProblemsMap::<T>::insert(number, Problem {
                number,
                reward,
                resolver: None,
                solution: None,
                submitter: who.clone(),
            });

            // Emit an event.
            Self::deposit_event(RawEvent::ProblemSubmited(number, reward, who));

            // Return a successful DispatchResult
            Ok(())
        }

        /// Resolve a problem
        /// 1 read & 5 write to the db
        #[weight = T::DbWeight::get().reads_writes(1, 5) + 10_000]
        pub fn resolve_problem(origin, number: u128, a: u128, b: u128) -> dispatch::DispatchResult {
            let who = ensure_signed(origin)?;

            let problem = match ProblemsMap::<T>::get(number) {
                None => {
                    return Err(Error::<T>::InexistentNumber.into())
                }
                Some(problem) if problem.resolver.is_some() => {
                    return Err(Error::<T>::AlreadyResolved.into())
                },
                Some(problem) => problem,
            };

            let multiplied = match a.checked_mul(b) {
                Some(n) => n,
                _ => return Err(Error::<T>::WrongAnswer.into())
            };

            // Check if the solution is correct
            if multiplied != number || !is_prime(a) || !is_prime(b) {
                return Err(Error::<T>::WrongAnswer.into());
            }

            // Unreserve the reward
            T::Currency::unreserve(&problem.submitter, problem.reward);

            // Make a 80/20 ratio of the reward
            let imbalance = T::Currency::burn(problem.reward);
            let (to_resolver, to_treasury) = imbalance.ration(80, 20);

            // Transfer 80% to the resolver
            T::Currency::transfer(&problem.submitter, &who, to_resolver.peek(), ExistenceRequirement::KeepAlive)?;

            // Transfer 20% to pallet treasury
            T::Currency::transfer(&problem.submitter, &Self::account_id(), to_treasury.peek(), ExistenceRequirement::KeepAlive)?;

            // Set the problem as resolved
            ProblemsMap::<T>::mutate(number, |_| {
                Problem::<T> {
                    number,
                    reward: problem.reward,
                    solution: Some((a, b)),
                    resolver: Some(who.clone()),
                    submitter: problem.submitter,
                }
            });

            // Emit an event.
            Self::deposit_event(RawEvent::ProblemResolved(number, a, b, who));

            // Return a successful DispatchResult
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::is_prime;

    // First few prime numbers
    const PRIMES: &[u128] = &[
        2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89,
        97,
    ];

    #[test]
    fn test_prime() {
        for n in 0..100 {
            assert_eq!(is_prime(n), PRIMES.contains(&n));
        }
    }
}
