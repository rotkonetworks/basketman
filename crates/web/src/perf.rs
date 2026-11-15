//! A tiny frame profiler for finding what a slow frame is spent on.
//! `SHOT_PERF=1` with the screenshot harness prints the totals on exit.
//! Does nothing in the browser build.

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::time::Instant;

    thread_local! {
        static ACC: RefCell<BTreeMap<&'static str, (f64, u32)>> = const { RefCell::new(BTreeMap::new()) };
    }

    pub struct Scope(&'static str, Instant);

    pub fn scope(name: &'static str) -> Scope {
        Scope(name, Instant::now())
    }

    impl Drop for Scope {
        fn drop(&mut self) {
            let ms = self.1.elapsed().as_secs_f64() * 1000.0;
            ACC.with(|a| {
                let mut a = a.borrow_mut();
                let e = a.entry(self.0).or_insert((0.0, 0));
                e.0 += ms;
                e.1 += 1;
            });
        }
    }

    /// Add a measurement made elsewhere, in milliseconds.
    pub fn add(name: &'static str, ms: f64) {
        ACC.with(|a| {
            let mut a = a.borrow_mut();
            let e = a.entry(name).or_insert((0.0, 0));
            e.0 += ms;
            e.1 += 1;
        });
    }

    /// Forget what was measured so far, to leave start-up out.
    pub fn reset() {
        ACC.with(|a| a.borrow_mut().clear());
    }

    pub fn report() {
        ACC.with(|a| {
            for (name, (ms, n)) in a.borrow().iter() {
                eprintln!(
                    "perf {name:<22} {:>8.3} ms avg over {n} calls",
                    ms / *n as f64
                );
            }
        });
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    pub struct Scope;
    pub fn scope(_: &'static str) -> Scope {
        Scope
    }
    pub fn add(_: &'static str, _: f64) {}
    pub fn reset() {}
    pub fn report() {}
}

pub(crate) use imp::{add, report, reset, scope};
