use parity_codec::Encode;
use system::ensure_signed;
use support::{decl_storage, decl_module, StorageValue, StorageMap, dispatch::Result, ensure};
use runtime_primitives::traits::{As, Hash};

#[derive(Encode, Decode, Default, Clone, PartialEq)]
pub struct Kitty<Hash, Balance> {
    id: Hash,
    dna: Hash,
    price: Balance,
    gen: u64,
}

pub trait Trait: balances::Trait {}

// decl_strorageマクロの適用
decl_storage! {
    trait Store for Module<T: Trait> as KittyStorage {
        // Declare storage and its getter functions here.
        Kitties get(kitty): map T::Hash => Kitty<T::Hash, T::Balance>; // hash value => kitty
        KittyOwner get(owner_of): map T::Hash => Option<T::AccountId>; // hash value => account ID
        OwnedKitty get(kitty_of_owner): map T::AccountId => T::Hash;   // account ID => hash value

        Nonce: u64;
    }
}
// decl_moduleマクロの適用
decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        // Declare public functions here.

        // 新しいKittyを生成し、その成否を返す関数を定義する。
        fn create_kitty(origin) -> Result {
            // create_kitty()を叩いたsenderの正当性を確認する。
            let sender = ensure_signed(origin)?;

            // nonceを計算する。
            let nonce = <Nonce<T>>::get();

            // creat_kitty()を叩いたsenderからnonceと合わせてハッシュ値を計算する。
            // 「random_hash <--> kitty」は一対一対応している。
            let random_hash = (<system::Module<T>>::random_seed(), &sender, nonce)
                .using_encoded(<T as system::Trait>::Hashing::hash);

            // 計算したrandom_hashが衝突していないことを確認する。
            ensure!(!<KittyOwner<T>>::exists(random_hash), "the kitty coressponding to this ID already exit!");

            // new_kittyを生成する。
            let new_kitty = Kitty {
                id: random_hash,
                dna: random_hash,
                price: <T::Balance as As<u64>>::sa(0),
                gen: 0,
            };

            // (random_hash, new_kitty)を登録する。
            <Kitties<T>>::insert(random_hash, new_kitty);

            // (生成者を一意に区別するハッシュ値, 生成者)を登録する。
            <KittyOwner<T>>::insert(random_hash, &sender);

            // (生成者, 生成者を一意に区別するハッシュ値)を登録する。
            <OwnedKitty<T>>::insert(&sender, random_hash);

            // Nonceをインクリメント
            <Nonce<T>>::mutate(|n| {
                *n += 1
            });

            Ok(())
        }
    }
}
