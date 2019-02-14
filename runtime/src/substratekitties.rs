use parity_codec::Encode;
use system::ensure_signed;
use support::{decl_storage, decl_module, StorageValue, StorageMap, dispatch::Result, ensure, decl_event};
use runtime_primitives::traits::{As, Hash, Zero};
use rstd::cmp;

// Substrateでは「あるトランザクションがFinalizeされたことが、直接そのトランザクションによって実行される
// 関数が成功裏に終わったこと」を意味しない。Substrateでは「呼び出された関数が成功裏に終わったこと」を
// Eventというものを明示的に返すことで表現する。Eventには任意の型を与えることができる。
// Eventの役割は「その関数の実行の成否を報告すること」と
// 「Off-chainの世界に、ブロックチェーン上で状態遷移が発生したことを宣言すること」である。
// Eventの定義にはdecl_eventマクロを使うと簡単にできるようになっている。

// Ethereum上のコントラクトを開発するのではなく、substrateでチェーンのロジックを開発するのだから、
// ブロックチェーンの状態を変化させうるあらゆる事柄に対して注意を払わないといけない。
// substrateではリスト型をプリミティブな型として提供していない。
// 何故ならばリスト型は予期せず危険な動作を引き起こす可能性があるからである。
// 例えば「リストの要素を一つずつイテレーションしていく」という操作は、最悪の場合O(n)の計算量を必要とすることに
// なりかねない。
// そこでsubstrate上でリストのようなデータ構造を実現したいならば、マッピングを利用して実装する必要がある。
// リストの操作は注意が必要である。具体的にはoverflow/underflowしないように注意する。幸いなことに、
// rustは型安全な演算が言語機能として提供されている。もしくはrustのResult型を使うことでも対応できる。
// Verify first, write lastの原則：安全な操作であることを確認してから、ブロックに書き込む。

// mapによるリストのエミュレートだと「アカウントとkittyが一対一対応」する必要があるので、複数のkittiesを一人が
// 所有することができない。この問題はタプルを使うことで解決させることができる。

// 「データをブロックチェーンから引き出して、更新する」という操作はverify first, write lastの原則を
// 適用することが求められる。

// kittyの所有権の変更はSwap and Popメソッドで行う。

pub trait Trait: balances::Trait {
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

#[derive(Encode, Decode, Default, Clone, PartialEq)]
pub struct Kitty<Hash, Balance> {
    id: Hash,       // idでkittyを唯一に識別する。
    dna: Hash,      // 個体に固有の値なのでdnaとして機能する。
    price: Balance, // 価格
    gen: u64,       // 性別。gender。
}

// decl_eventマクロの適用によってブロックチェーンの状態遷移後に返されるイベントの型を定義する。
decl_event!(
    pub enum Event<T>
        where <T as system::Trait>::AccountId,
              <T as system::Trait>::Hash,
              <T as balances::Trait>::Balance
    {
        Created(AccountId, Hash),                // `AccountId`が`Hash`で指し示されるkittyをcreateした。
        PriceSet(AccountId, Hash, Balance),      // `AccountId`が`Hash`で指し示されるkittyのpriceを`Balance`に設定した。
        Transferred(AccountId, AccountId, Hash), // `AccountId`が`AccountId`に`Hash`で指し示されるkittyをtransferした。
        Bought(AccountId, AccountId, Hash, Balance),   // `AccountId`が`AccountId`から`Hash`で指し示されるkittyを`Balance`buyした。
    }
);

// decl_storageマクロの適用によってチェーンに刻むデータ構造を定義する。
decl_storage! {
    trait Store for Module<T: Trait> as KittyStorage {
        // Declare storage and its getter functions here.

        // hash value is a unique key to each kitty.
        Kitties get(kitty): map T::Hash => Kitty<T::Hash, T::Balance>; // hash value => kitty
        KittyOwner get(owner_of): map T::Hash => Option<T::AccountId>; // hash value => account ID

        AllKittiesArray get(kitty_by_index): map u64 => T::Hash;       // kitty's index => hash value
        AllKittiesCount get(all_kitties_count): u64;                   // how many kitties exist?
        AllKittiesIndex: map T::Hash => u64;                           // hash value => kitty's index

        // OwnedKitty get(kitty_of_owner): map T::AccountId => T::Hash;   // account ID => hash value
        OwnedKittiesArray get(kitty_of_owner_by_index): map (T::AccountId, u64) => T::Hash; // (account ID, the index of owned kitty) => hash value
        OwnedKittiesCount get(owned_kitty_count): map T::AccountId => u64; // account ID => count of owned kitties
        OwnedKittiesIndex: map T::Hash => u64; // そのkittyが所有者にとって何番目のkittyなのかを返す。

        Nonce: u64;
    }
}
// decl_moduleマクロの適用によってチェーンに刻むデータへのアクセスインタフェースの実装を記述する。
decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        // Declare public functions here.

        // トランザクションの執行後にイベントを吐く関数をデフォルトの挙動で定義する。
        fn deposit_event<T>() = default;

        // 新しいKittyを生成し、その成否を返す関数を定義する。
        // Kittyたちはリストのような見た目のデータ構造でアカウントに紐づけられた形で管理される。
        fn create_kitty(origin) -> Result {

            // Verify first, write lastの原則：create_kitty()を叩いたsenderの正当性を確認する。
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

            // 新たに生成されたkittyを記録する。
            Self::_mint(sender, random_hash, new_kitty)?;

            // Nonceをインクリメント
            <Nonce<T>>::mutate(|n| {
                *n += 1
            });

            Ok(())
        }

        // kittyのIDと新しいpriceを与えて、kittyのpriceを更新する関数を定義する。
        fn set_price(origin, kitty_id: T::Hash, new_price: T::Balance) -> Result {

            // Verify first, write lastの原則：create_kitty()を叩いたsenderの正当性を確認する。
            let sender = ensure_signed(origin)?;

            // Verify first, write lastの原則：指定したkittyが存在することを確認する。
            ensure!(<Kitties<T>>::exists(kitty_id), "Error: invalid kitty id: this kitty does not exist");

            // Verify first, write lastの原則：本当にそのkittyはあなたのもの？
            let owner = Self::owner_of(kitty_id).ok_or("Error: there is no owner for this kitty")?; // そもそも所有者のいないkittyだった。
            ensure!(owner == sender, "Error: you have no ownership to this kitty"); // あなたのkittyではなかった。

            // kittyをkitty IDで引き出して、priceを更新して、書き戻す。
            let mut kitty = Self::kitty(kitty_id);
            kitty.price = new_price;
            <Kitties<T>>::insert(kitty_id, kitty);

            // ブロックチェーンの状態が遷移したので、それを通知するイベントを吐く。
            Self::deposit_event(RawEvent::PriceSet(sender, kitty_id, new_price));

            Ok(())
        }

        // 呼び出し側が転送先を指定してkittyを転送し、その成否を返す関数を定義する。
        fn transfer(origin, to: T::AccountId, kitty_id: T::Hash) -> Result {

            // Verify first, write lastの原則：正当なユーザーがこの関数を叩いたかを確認する。
            let sender = ensure_signed(origin)?;

            // Verify first, write lastの原則：転送したいkittyの存在を確認する。
            let owner = Self::owner_of(kitty_id).ok_or("Error: there is no owner for this kitty")?;
            ensure!(owner == sender, "Error: you have no ownership for this kitty");

            // 転送をする。
            Self::_transfer_from(sender, to, kitty_id)?;

            Ok(())
        }

        // 呼び出し側が買いたいkittyのIDと買取額を引数に与えて、購入を実行し、その成否を返す関数を定義する。
        fn buy_kitty(origin, kitty_id: T::Hash, max_price: T::Balance) -> Result {

            // Verify first, write lastの原則：正当なユーザーがこの関数を叩いたかを確認する。
            let sender = ensure_signed(origin)?;

            // Verify first, write lastの原則：買いたいkittyが存在することを確認する。
            ensure!(<Kitties<T>>::exists(kitty_id), "Error: invalid kitty id: this kitty does not exist");

            // Verify first, write lastの原則：kittyの所有者が正当であることを確認する。
            let owner = Self::owner_of(kitty_id).ok_or("Error: there is no owner for this kitty")?;
            ensure!(owner != sender, "Error: you can not buy your own kitty");

            // 売買されるkittyを引き出す。
            let mut kitty = Self::kitty(kitty_id);
            // 売却額を確認する。
            let kitty_price = kitty.price;

            // 売却額 == 0のkittyは売却対象ではないものとする。
            ensure!(!kitty_price.is_zero(), "Error: this kitty you want to buy is not for sale");

            // 買取側の口座残高が売却額以下でないと買えないので確認する。
            ensure!(kitty_price <= max_price, "Error: this kitty you want to buy costs more than your max price");

            // 双方の残高をアトミックに更新する。
            <balances::Module<T>>::make_transfer(&sender, &owner, kitty_price)?;

            // kittyを売却側から購入側へ転送する。
            Self::_transfer_from(owner.clone(), sender.clone(), kitty_id)?;

            kitty.price = <T::Balance as As<u64>>::sa(0);

            <Kitties<T>>::insert(kitty_id, kitty);

            // Boughtイベントを吐く。
            Self::deposit_event(RawEvent::Bought(sender, owner, kitty_id, kitty_price));

            Ok(())
        }

        // 親となる二匹を引数として与えて、子供を作らせ、その成否を返す関数。
        fn breed_kitty(origin, kitty_id_1: T::Hash, kitty_id_2: T::Hash) -> Result {

            // Verify first, write lastの原則：正当なユーザーがこの関数を叩いたかを確認する。
            let sender = ensure_signed(origin)?;

            // Verify first, write lastの原則：kittyの存在確認。
            ensure!(<Kitties<T>>::exists(kitty_id_1), "Error: this cat 1 does not exist");
            ensure!(<Kitties<T>>::exists(kitty_id_2), "Error: this cat 2 does not exist");

            // 子供に振られるidを計算する。
            let nonce = <Nonce<T>>::get();
            let random_hash = (<system::Module<T>>::random_seed(), &sender, nonce)
                                .using_encoded(<T as system::Trait>::Hashing::hash);

            // 親を引き出す。
            let kitty_1 = Self::kitty(kitty_id_1);
            let kitty_2 = Self::kitty(kitty_id_2);

            // 最終的な子供のDNA（初期値として片親のDNAをコピー）
            let mut final_dna = kitty_1.dna;

            // DNAをシェイキング！
            for (i, (dna_2_element, r)) in kitty_2.dna.as_ref().iter().zip(random_hash.as_ref().iter()).enumerate() {
                if r % 2 == 0 {
                    final_dna.as_mut()[i] = *dna_2_element;
                }
            }

            // 子供誕生
            let new_kitty = Kitty {
                id: random_hash,
                dna: final_dna,
                price: <T::Balance as As<u64>>::sa(0),
                gen: cmp::max(kitty_1.gen, kitty_2.gen) + 1,
            };

            // 子供の所有権を記録する。
            Self::_mint(sender, random_hash, new_kitty)?;

            // nonce更新
            <Nonce<T>>::mutate(|n| *n += 1);

            Ok(())

        }
    }
}

impl <T: Trait> Module<T> {

    // 新たなkittyを記録するヘルパー関数を用意。
    fn _mint(to: T::AccountId, kitty_id: T::Hash, new_kitty: Kitty<T::Hash, T::Balance>) -> Result {
        // 計算したrandom_hashが衝突していないことを確認する。
        ensure!(!<KittyOwner<T>>::exists(kitty_id), "Error: the kitty coressponding to this ID already exit!");

        // Verify first, write lastの原則：この人が現在何匹のkittyを所有しているかを取得する。
        let owned_kitty_count = Self::owned_kitty_count(&to);

        // Verify first, write lastの原則：新しいkittyを所有するので更新する。
        let new_owned_kitty_count = owned_kitty_count.checked_add(1)
            .ok_or("Error: Overflow happed when trying to register a new kitty in your account balance")?;

        // Verify first, write lastの原則：現在登録されているkittiesの個体数を確認する。
        let all_kitties_count = Self::all_kitties_count();

        // Verify first, write lastの原則：これから登録しようとしているkittyを追加してoverflowしないかを確認する。
        let new_all_kitties_count = all_kitties_count.checked_add(1)
            .ok_or("Error: Overflow happened when trying to register a new kitty")?;

        // (random_hash, new_kitty)を登録する。
        <Kitties<T>>::insert(kitty_id, new_kitty);

        // (生成者を一意に区別するハッシュ値, 生成者)を登録する。
        <KittyOwner<T>>::insert(kitty_id, &to);

        // (all_kitties_count, random_hash)を登録する。all_kitties_countは0オリジンの通し番号となる。
        <AllKittiesArray<T>>::insert(all_kitties_count, kitty_id);

        // 「現在のkittiesの個体数」を更新する。
        <AllKittiesCount<T>>::put(new_all_kitties_count);

        // (random_hash, all_kitties_count)を登録する。
        <AllKittiesIndex<T>>::insert(kitty_id, all_kitties_count);

        // // (生成者, 生成者を一意に区別するハッシュ値)を登録する。
        // <OwnedKitty<T>>::insert(&sender, random_hash);

        // ((ユーザー, その人にとって何匹目か), kittyの識別子)を記録する。
        // こうすることで、二次元リストをエミュレートする。「誰の」「何番目か」で一匹を指定できる。
        <OwnedKittiesArray<T>>::insert((to.clone(), owned_kitty_count), kitty_id);

        // (ユーザー, ユーザーの所有しているkittyの個体数)を登録する。
        <OwnedKittiesCount<T>>::insert(&to, new_owned_kitty_count);

        // 今生成されたkittyが、その所有者にとって何番目のkittyなのかを登録する。
        <OwnedKittiesIndex<T>>::insert(kitty_id, owned_kitty_count);

        // トランザクション執行後のイベントを吐く。
        Self::deposit_event(RawEvent::Created(to, kitty_id));

        Ok(())
    }

    // 転送元と転送先、転送されるkittyを特定するハッシュ値を引数に、転送を実行しその成否を返すヘルパー関数
    fn _transfer_from(from: T::AccountId, to: T::AccountId, kitty_id: T::Hash) -> Result {

        // Verify first, write lastの原則：呼び出し元が転送したいkittyの所有者であるかを確認する。
        let owner = Self::owner_of(kitty_id).ok_or("Error: there is no owner for this kitty")?;
        ensure!(owner == from, "Error: `from` account have no ownership for this kitty");

        // 所有者の中の何番目のkittyを転送したいのかを確認する。
        let owned_kitty_count_from = Self::owned_kitty_count(&from);

        // 転送先では何番目のkittyとして扱われるのかを確認する。
        let owned_kitty_count_to = Self::owned_kitty_count(&to);

        // 転送先がすでにn匹のkittyを所有しているならば、転送先ではn+1匹目として扱われることを確認する。
        let new_owned_kitty_count_to = owned_kitty_count_to.checked_add(1)
            .ok_or("Error: happend overflow of `to`'s kitty balance while executing transfer method")?;

        // 転送元がn匹のkittyを所有しているならば、転送してしまうと所有している個体数が1減ることを確認する。
        let new_owned_kitty_count_from = owned_kitty_count_from.checked_sub(1)
            .ok_or("Error: happend underflow of `from`'s kitty balance while executing transfer method")?;

        // 転送されるkittyが転送前の所有者にとって何番目の個体なのかを確認する。
        let kitty_index = <OwnedKittiesIndex<T>>::get(kitty_id);

        // Swap and Popメソッドで転送を実施する。
        // 転送したいkittyが転送前所有者の最直近に得た個体だったらPopすればいい。
        // そうでないならば、転送されるkittyの位置に、最直近で得た個体へのポインタを貼る（Swap）。
        if kitty_index != new_owned_kitty_count_from {

            // 転送元が最直近に所有権を得たkittyのインデックスを確認する。
            let last_kitty_id = <OwnedKittiesArray<T>>::get((from.clone(), new_owned_kitty_count_from));

            <OwnedKittiesArray<T>>::insert((from.clone(), kitty_index), last_kitty_id);
            <OwnedKittiesIndex<T>>::insert(last_kitty_id, kitty_index);
        }

        // 転送されたkittyの所有者を更新する。
        <KittyOwner<T>>::insert(&kitty_id, &to);

        // 転送されたkittyは所有者にとって何番目であるかが変更されたので更新する。
        <OwnedKittiesIndex<T>>::insert(kitty_id, owned_kitty_count_to);

        // 転送した側は転送して個体数が減ったので更新する。\
        <OwnedKittiesArray<T>>::remove((from.clone(), new_owned_kitty_count_from));

        // 転送された側は持っている個体数が増えたので更新する。
        // `to`の`owned_kitty_count_to`匹目が`kitty_id`という意味である。
        <OwnedKittiesArray<T>>::insert((to.clone(), owned_kitty_count_to), kitty_id);

        //  双方の持っている個体数を更新する。
        <OwnedKittiesCount<T>>::insert(&from, new_owned_kitty_count_from);
        <OwnedKittiesCount<T>>::insert(&to, new_owned_kitty_count_to);

        // Transferredイベントを吐く。
        Self::deposit_event(RawEvent::Transferred(from, to, kitty_id));

        Ok(())
    }
}