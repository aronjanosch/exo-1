//! Rules for the shield: the regen after the recovery delay, the overflow to the hull, and the
//! quantum shield's drop and offline time.
use super::*;

#[test]
fn regen_scales_with_power() {
    // The gain of one second at `power` after the recovery delay has run out (the delay's own
    // second at power 0 gives nothing).
    let gain = |power: f64| {
        let mut s = Shield::new(ShieldTuning::default());
        s.absorb(500.0);
        s.step(3.0, 0.0);
        let before = s.current();
        s.step(1.0, power);
        s.current() - before
    };
    let full = gain(1.0);
    assert!((full - 50.0).abs() < 1e-9, "full power gives the charge rate, got {full}");
    assert!((gain(0.5) - full / 2.0).abs() < 1e-9, "half power gives half the regen");
}

#[test]
fn absorb_returns_the_overflow() {
    let mut s = Shield::new(ShieldTuning::default());
    assert_eq!(s.absorb(300.0), 0.0);
    assert!((s.current() - 700.0).abs() < 1e-9, "the shield took the hit, {}", s.current());
    // 700 left: 100 of an 800 hit goes on to the hull.
    assert!((s.absorb(800.0) - 100.0).abs() < 1e-9);
    assert_eq!(s.current(), 0.0);

    // Deflection 0.2 ignores a fifth of each hit: 1000 arrives as 800.
    let mut d = Shield::new(ShieldTuning { deflection: 0.2, ..ShieldTuning::default() });
    assert_eq!(d.absorb(1000.0), 0.0);
    assert!((d.current() - 200.0).abs() < 1e-9);
    assert!((d.absorb(1000.0) - 600.0).abs() < 1e-9, "800 arrives, 200 is taken, 600 passes");
}

#[test]
fn quantum_drops_below_75_and_stays_down_for_the_offline_time() {
    let mut s = Shield::new(ShieldTuning::default());
    s.absorb(200.0);
    assert!(s.quantum_up(), "80 % is up");
    s.absorb(100.0);
    assert!(!s.quantum_up(), "70 % is down");
    // Regen refills the shield to 85 % in the first 3 s, but the quantum shield stays down for the
    // 30 s offline time counted from the drop.
    s.step(3.0, 1.0);
    assert!(s.fraction() > 0.75, "the shield is back above 75 %");
    assert!(!s.quantum_up(), "still down while the offline time runs");
    s.step(26.0, 1.0);
    assert!(!s.quantum_up(), "29 s after the drop");
    s.step(1.0, 1.0);
    assert!(s.quantum_up(), "up at 30 s");
}

#[test]
fn no_regen_inside_the_recovery_delay() {
    let mut s = Shield::new(ShieldTuning::default());
    s.absorb(300.0);
    s.step(2.0, 1.0);
    assert!((s.current() - 700.0).abs() < 1e-9, "no regen in the first 3 s, got {}", s.current());
    s.step(2.0, 1.0);
    assert!(s.current() > 700.0, "regen after the delay");
    // A new hit starts the delay again.
    let before = s.current();
    s.absorb(100.0);
    s.step(2.0, 1.0);
    assert!((s.current() - (before - 100.0)).abs() < 1e-9, "the delay restarts with the hit");
}
