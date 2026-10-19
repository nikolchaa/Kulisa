//! Which screens read which signals. A screen's reads are recorded while it is rebuilt;
//! a change to a signal marks the screens that read it last time dirty.

use std::collections::{BTreeSet, HashMap};

#[derive(Default)]
pub struct Deps {
    readers: HashMap<u64, BTreeSet<String>>,
    building: Vec<String>,
    dirty: BTreeSet<String>,
}

impl Deps {
    /// Starts recording `screen`'s reads, forgetting the ones from its last build.
    pub fn begin(&mut self, screen: &str) {
        for r in self.readers.values_mut() {
            r.remove(screen);
        }
        self.dirty.remove(screen);
        self.building.push(screen.to_string());
    }

    pub fn end(&mut self) {
        self.building.pop();
    }

    /// A signal was read (by the screen being built, if any).
    pub fn read(&mut self, signal: u64) {
        if let Some(s) = self.building.last() {
            self.readers.entry(signal).or_default().insert(s.clone());
        }
    }

    /// A signal changed.
    pub fn changed(&mut self, signal: u64) {
        if let Some(r) = self.readers.get(&signal) {
            self.dirty.extend(r.iter().cloned());
        }
    }

    /// Marks a screen for rebuilding.
    pub fn mark(&mut self, screen: &str) {
        self.dirty.insert(screen.to_string());
    }

    /// The screens to rebuild, in name order.
    pub fn take_dirty(&mut self) -> Vec<String> {
        std::mem::take(&mut self.dirty).into_iter().collect()
    }

    /// Forgets a screen (it was hidden).
    pub fn forget(&mut self, screen: &str) {
        for r in self.readers.values_mut() {
            r.remove(screen);
        }
        self.dirty.remove(screen);
    }

    /// Forgets a signal (it was garbage collected).
    pub fn drop_signal(&mut self, signal: u64) {
        self.readers.remove(&signal);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_readers_go_dirty() {
        let mut d = Deps::default();
        d.begin("say");
        d.read(1);
        d.read(2);
        d.end();
        d.begin("menu");
        d.read(2);
        d.end();
        d.read(3); // outside a build: not recorded
        d.changed(1);
        assert_eq!(d.take_dirty(), ["say"]);
        d.changed(2);
        assert_eq!(d.take_dirty(), ["menu", "say"]);
        d.changed(3);
        assert!(d.take_dirty().is_empty());
        // A rebuild that no longer reads 2 stops listening to it.
        d.begin("menu");
        d.end();
        d.changed(2);
        assert_eq!(d.take_dirty(), ["say"]);
        d.forget("say");
        d.changed(1);
        assert!(d.take_dirty().is_empty());
    }
}
