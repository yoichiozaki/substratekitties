use system::ensure_signed;
use support::{decl_storage, decl_module, StorageValue, dispatch::Result};

pub trait Trait: system::Trait {}

decl_storage! {
    trait Store for Module<T: Trait> as KittyStorage {
        // Declare storage and its getter functions here.
        Value: u64;
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        // Declare public functions here.
        fn set_value(origin, value: u64) -> Result {
            let _sender = ensure_signed(origin)?;
            <Value<T>>::put(value);
            Ok(())
        }
    }
}
