use system::ensure_signed;
use support::{decl_storage, decl_module, StorageMap, dispatch::Result};
use runtime_primitives::traits::{As, Hash};

#[derive(Encode, Decode, Default, Clone, PartialEq)]
pub struct Kitty<Hash, Balance> {
    id: Hash,
    dna: Hash,
    price: Balance,
    gen: u64,
}

pub trait Trait: balances::Trait {}

decl_storage! {
    trait Store for Module<T: Trait> as KittyStorage {
        // Declare storage and its getter functions here.
        OwnedKitty: map T::AccountId => Kitty<T::Hash, T::Balance>;
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        // Declare public functions here.
        fn create_kitty(origin) -> Result {
            let sender = ensure_signed(origin)?;

            let new_kitty = Kitty {
                id: <T as system::Trait>::Hashing::hash_of(&0),
                dna: <T as system::Trait>::Hashing::hash_of(&0),
                price: <T::Balance as As<u64>>::sa(0),
                gen: 0,
            };

            <OwnedKitty<T>>::insert(&sender, new_kitty);

            Ok(())
        }
    }
}
