//! Shops of #230: offers say what is available, affordable and owned; a purchase gives the
//! unlock (the progress charges it) or the goods (a wallet change and `GoodsBought`); refusals
//! name the reason.
mod common;

use common::*;
use gameplay_core::shop::{OfferView, Refusal as ShopRefusal, Shop, Shops};
use gameplay_core::{CommodityId, LocationId, TextKey, UnlockId, WorldEvent};

/// The inline shops: the mechanic at Bent Spoon (an unlock and two goods), and a shop in the
/// back alley that opens after the first job.
const SHOPS: &[(&str, &str)] = &[
    (
        "shop/mechanic.json",
        r#"{"id":"mechanic","name":"shop.mechanic.name","location":"bent_spoon","items":[
            {"unlock":"hidden_hold"},
            {"goods":"fizzy_mud","amount":3,"price":90},
            {"goods":"sock_dust","amount":2,"price":50,"condition":{"track_at_least":{"track":"jobs_done","value":1}}}
        ]}"#,
    ),
    (
        "shop/alley.json",
        r#"{"id":"alley","name":"shop.alley.name","location":"back_alley","items":[{"goods":"fizzy_mud","amount":1,"price":40}]}"#,
    ),
];

fn shops(k: &gameplay_core::Content) -> Shops {
    Shops::load(&fixture_files(&fixture(SHOPS)), k).unwrap_or_else(|e| panic!("{e:#?}"))
}

fn mechanic(w: &World) -> Shop {
    let s = shops(&w.k);
    s.shops.get("mechanic").unwrap().record.clone()
}

fn offers(w: &World, shop: &Shop) -> Vec<OfferView> {
    shop.offers(&w.progress, &w.k)
}

#[test]
fn offers_say_what_is_available_affordable_and_owned() {
    let mut w = World::fixture();
    let shop = mechanic(&w);
    let o = offers(&w, &shop);
    assert_eq!(o.len(), 3);
    assert_eq!(o[0], OfferView { label: TextKey::new("unlock.hidden_hold.name"), price: 400, amount: None, available: true, affordable: false, owned: false });
    assert_eq!(o[1], OfferView { label: TextKey::new("commodity.fizzy_mud.name"), price: 90, amount: Some(3), available: true, affordable: true, owned: false }, "100 credits buy one 90 crate");
    assert!(!o[2].available, "sock dust waits for a job");

    w.send(track("wallet", 400));
    assert!(offers(&w, &shop)[0].affordable, "400 credits now buy the hold");

    w.send(job_done("first_haul"));
    assert!(offers(&w, &shop)[2].available, "the job opened the sock dust");
}

#[test]
fn buying_an_unlock_gives_it_and_charges_once() {
    let mut w = World::fixture();
    let shop = mechanic(&w);
    w.send(track("wallet", 400));
    let events = shop.buy(0, &w.progress, &w.k, LocationId::new("bent_spoon")).unwrap();
    assert_eq!(events, vec![WorldEvent::UnlockBought { unlock: UnlockId::new("hidden_hold") }], "the progress charges the price, the shop does not");
    w.send_all(events);
    assert_eq!(w.wallet(), 100, "500 - 400");
    assert!(w.progress.has_tag(&gameplay_core::Tag::new("ship_hidden_hold")));
    assert!(offers(&w, &shop)[0].owned);
    assert_eq!(shop.buy(0, &w.progress, &w.k, LocationId::new("bent_spoon")), Err(ShopRefusal::Owned));
}

#[test]
fn buying_goods_charges_the_wallet_and_names_the_goods() {
    let mut w = World::fixture();
    let shop = mechanic(&w);
    let events = shop.buy(1, &w.progress, &w.k, LocationId::new("bent_spoon")).unwrap();
    assert_eq!(
        events,
        vec![track("wallet", -90), WorldEvent::GoodsBought { commodity: CommodityId::new("fizzy_mud"), amount: 3, at: LocationId::new("bent_spoon") }]
    );
    w.send_all(events);
    assert_eq!(w.wallet(), 10);
}

#[test]
fn purchases_are_refused_with_the_reason() {
    let mut w = World::fixture();
    let shop = mechanic(&w);
    let bent = LocationId::new("bent_spoon");
    assert_eq!(shop.buy(0, &w.progress, &w.k, LocationId::new("drip_rock")), Err(ShopRefusal::WrongLocation), "not this shop's counter");
    assert_eq!(shop.buy(9, &w.progress, &w.k, bent.clone()), Err(ShopRefusal::NoSuchItem));
    assert_eq!(shop.buy(0, &w.progress, &w.k, bent.clone()), Err(ShopRefusal::NotEnoughMoney { price: 400, wallet: 100 }));
    assert_eq!(shop.buy(2, &w.progress, &w.k, bent.clone()), Err(ShopRefusal::ConditionNotMet), "sock dust waits for a job");
    assert_eq!(w.wallet(), 100, "no refusal changed anything");

    let alley = shops(&w.k).shops.get("alley").unwrap().record.clone();
    assert_eq!(alley.buy(0, &w.progress, &w.k, LocationId::new("back_alley")), Err(ShopRefusal::Closed), "the alley opens after a job");
    w.send(job_done("first_haul"));
    assert!(alley.buy(0, &w.progress, &w.k, LocationId::new("back_alley")).is_ok());
}

#[test]
fn shop_loader_names_file_and_field() {
    let k = kernel();
    let load = |text: &str| -> Vec<String> {
        let mut f = fixture_files(&[]);
        f.push(gameplay_core::File::new("shop/bad.json", text));
        Shops::load(&f, &k).unwrap_err()
    };
    let has = |e: &[String], what: &str| e.iter().any(|x| x.contains("shop/bad.json") && x.contains(what));
    let shop = |items: &str, location: &str| format!(r#"{{"id":"bad","name":"shop.bad.name","location":"{location}","items":[{items}]}}"#);

    assert!(has(&load(&shop(r#"{"unlock":"hidden_hold","goods":"fizzy_mud"}"#, "bent_spoon")), "not both"));
    assert!(has(&load(&shop(r#"{"amount":2}"#, "bent_spoon")), "unlock' or 'goods"));
    assert!(has(&load(&shop(r#"{"goods":"fizzy_mud","amount":2}"#, "bent_spoon")), "price"));
    assert!(has(&load(&shop(r#"{"goods":"fizzy_mud","amount":0,"price":5}"#, "bent_spoon")), "amount"));
    assert!(has(&load(&shop(r#"{"goods":"no_such","amount":1,"price":5}"#, "bent_spoon")), "no_such"));
    assert!(has(&load(&shop(r#"{"unlock":"no_such"}"#, "bent_spoon")), "no_such"));
    assert!(has(&load(&shop(r#"{"unlock":"hidden_hold"}"#, "nowhere")), "nowhere"));
    assert!(has(&load(&shop("", "bent_spoon")), "items"));
}
