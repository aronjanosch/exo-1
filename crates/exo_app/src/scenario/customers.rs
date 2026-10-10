//! #168, customers, headless: a customer's order turns into an offer at the wholesaler's counter
//! (through domain events), the crates are picked up there and set down at the customer's pad,
//! and the delivery moves the relationship, the next order and the customer's words. A second
//! order is ruined on purpose. The walk between the pads is the courier scenario's; here the
//! crates go there by test hook, as in `deliver`.
use super::deliver::{crates_above, gp, goods_of, pad, show_offer, walker_on_pad};
use super::*;
use crate::gameplay::Gameplay;
use customers_core::{rhythm_factor, CustomerId};
use jobs_core::{JobEvent, JobState};

const WHO: &str = "mabel_snood";

fn rel(w: &World) -> f64 {
    gp(w).customers.relationship(&CustomerId::new(WHO))
}

fn order_offer(w: &World) -> Option<jobs_core::Job> {
    gp(w).jobs.all().find(|j| j.state == JobState::Offered && j.order.as_ref().is_some_and(|o| o.by == WHO)).cloned()
}

pub fn customers_steps(s: &mut Vec<Step>) {
    s.push(settle());
    s.push(Box::new(|w, c| {
        crate::scenario::cargo::clear_crates(w);
        begin(w, c, "customers: an order becomes an offer");
        walker_on_pad(w, "slosh_wholesale");
        let g = &mut w.resource_mut::<Gameplay>().customers;
        g.set_wait(&CustomerId::new(WHO), 0.5);
        g.set_wait(&CustomerId::new("captain_pip"), 1e9);
        g.set_wait(&CustomerId::new("moss_committee"), 1e9);
        true
    }));
    s.push(wait(2.0));
    s.push(Box::new(|w, c| {
        let offer = order_offer(w);
        check(c, offer.as_ref().is_some_and(|j| j.legs[0].from.as_str() == "slosh_wholesale" && j.legs[0].to.as_str() == "noodle_post" && j.legs[0].commodity.as_str() == "sock_dust"),
            format!("customers: Mabel's order is an offer from the wholesaler to her pad ({:?})", offer.as_ref().map(|j| (&j.legs[0].amount, j.order.as_ref().map(|o| o.reward)))));
        c.v.insert("reward", offer.and_then(|j| j.order.map(|o| o.reward)).unwrap_or(0) as f64);
        c.v.insert("rel0", rel(w));
        c.v.insert("wallet0", gp(w).progress.wallet() as f64);
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.5));
    s.push(show_offer("customer_order"));
    s.push(Box::new(|w, c| {
        let text = w.resource_mut::<Gameplay>().panel_text("[F] take");
        check(c, text.contains("Order for Mabel Snood") && text.contains("Mabel Snood") && text.contains("Slosh"), format!("customers: the counter shows the order in the customer's words ({})", text.replace('\n', " | ")));
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let g = goods_of(w);
        let at_wholesale = g.iter().filter(|(_, g)| g.on_pad.as_ref().is_some_and(|l| l.as_str() == "slosh_wholesale")).count();
        check(c, !g.is_empty() && at_wholesale == g.len() && g.iter().all(|(_, g)| g.commodity.as_str() == "sock_dust"), format!("customers: the sock dust waits on the wholesaler's pad ({} crates)", g.len()));
        true
    }));
    // The crates go to Mabel's pad.
    s.push(Box::new(|w, _| {
        walker_on_pad(w, "noodle_post");
        true
    }));
    s.push(settle());
    s.push(Box::new(|w, _| {
        crates_above(w, "noodle_post", 0.4);
        true
    }));
    s.push(wait(14.0));
    s.push(Box::new(|w, c| {
        begin(w, c, "customers: the delivery pleases");
        let g = gp(w);
        let paid = g.progress.wallet() as f64 - c.v["wallet0"];
        check(c, paid >= c.v["reward"] * 0.9 && paid <= c.v["reward"], format!("customers: the order's own reward is paid ({paid} of {})", c.v["reward"]));
        let (r0, r1) = (c.v["rel0"], rel(w));
        check(c, r1 > r0 + 0.4, format!("customers: Mabel likes the crew more ({r0:.2} to {r1:.2})"));
        let thanked = g.shown.iter().any(|l| l.key == "customer.mabel_snood.thanks");
        check(c, thanked, "customers: she says thanks in her own words".into());
        let next = g.customers.next_order_in(&CustomerId::new(WHO)).unwrap_or(f64::MAX);
        check(c, next <= 240.0 * rhythm_factor(r1) + 1.0, format!("customers: her next order is due within her rhythm at the new relationship ({next:.0} s, at most {:.0})", 240.0 * rhythm_factor(r1)));
        let placed = g.shown.iter().any(|l| l.key == "notice.order.placed");
        check(c, placed, "customers: the order was announced".into());
        c.v.insert("rel1", r1);
        end(w, c, format!("relationship {r1:.2}"));
        true
    }));
    // A second order, ruined.
    s.push(Box::new(|w, c| {
        begin(w, c, "customers: a ruined delivery lets her down");
        w.resource_mut::<Gameplay>().customers.set_wait(&CustomerId::new(WHO), 0.5);
        c.v.insert("wallet1", gp(w).progress.wallet() as f64);
        true
    }));
    s.push(wait(2.0));
    s.push(Box::new(|w, c| {
        let Some(j) = order_offer(w) else {
            check(c, false, "customers: the second order is on offer".into());
            return true;
        };
        w.resource_mut::<Gameplay>().push_job(crate::gameplay::HOST, JobEvent::OfferAccepted { job: j.id });
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, _| {
        // Dropped from a great height: wrecked.
        crates_above(w, "noodle_post", 30.0);
        true
    }));
    s.push(wait(14.0));
    s.push(Box::new(|w, c| {
        let g = gp(w);
        let (r1, r2) = (c.v["rel1"], rel(w));
        let grumbled = g.shown.iter().any(|l| l.key == "customer.mabel_snood.grumble");
        check(c, r2 < r1, format!("customers: wrecked goods lower the relationship ({r1:.2} to {r2:.2})"));
        check(c, grumbled, "customers: she grumbles in her own words".into());
        check(c, r2 > 0.0, "customers: but she is not lost for good".into());
        // Orders come again later (the wait was drawn again at the settlement).
        check(c, g.customers.next_order_in(&CustomerId::new(WHO)).is_some(), "customers: she will order again".into());
        let _ = pad(w, "noodle_post");
        end(w, c, format!("relationship {r2:.2}"));
        true
    }));
}
