use crate::app::{thread, Person};
use std::sync::{Arc, Condvar, Mutex};
use zearch::{Document, Index, Search};

pub struct SearchIndex {
    fuzzy: Index<'static>,
    exact: Vec<(&'static str, u32)>,
    #[allow(dead_code)]
    persons: Arc<Vec<Person>>,
}

impl Document<'_, 'static> for Person {
    fn name(&'_ self) -> &'static str {
        self.name
    }
}

impl SearchIndex {
    pub fn new(persons: Arc<Vec<Person>>) -> Self {
        log::info!("Initializing search engine");
        let fuzzy = Index::new_in_memory(&persons);
        log::info!("Fuzzy index initialized");
        let mut exact = Vec::with_capacity(persons.len());
        for (i, p) in persons.iter().enumerate() {
            exact.push((p.id, i as u32));
        }
        exact.sort_unstable_by_key(|(id, _)| *id);
        log::info!("Search engine initialized");
        SearchIndex {
            fuzzy,
            exact,
            persons,
        }
    }

    pub fn search(&self, query: &str, max_results: usize) -> Vec<u32> {
        let exact = self
            .exact
            .binary_search_by_key(&query, |(name, _)| *name)
            .ok();
        let mut fuzzy = self
            .fuzzy
            .search(Search::new(query).with_limit(max_results));
        if let Some(e) = exact {
            let exact_match = self.exact[e].1;
            if let Some(i) = fuzzy.iter().position(|&i| i == exact_match) {
                fuzzy.remove(i);
            }
            fuzzy.insert(0, exact_match);
        }
        fuzzy
    }
}

pub struct SearchEngine {
    inner: Arc<(Mutex<Option<SearchIndex>>, Condvar)>,
}

impl SearchEngine {
    pub fn new(persons: Arc<Vec<Person>>) -> Self {
        let inner = Arc::new((Mutex::new(None), Condvar::new()));
        let inner_clone = inner.clone();

        thread::spawn(move || {
            let engine = SearchIndex::new(persons);
            let (lock, cvar) = &*inner_clone;
            let mut state = lock.lock().unwrap();
            *state = Some(engine);
            cvar.notify_all();
        });

        SearchEngine { inner }
    }

    pub fn get_blocking<T>(&self, op: impl FnOnce(&SearchIndex) -> T) -> T {
        let (lock, cvar) = &*self.inner;
        let mut state = lock.lock().unwrap();
        while state.is_none() {
            state = cvar.wait(state).unwrap();
        }
        op(state.as_ref().unwrap())
    }
}
