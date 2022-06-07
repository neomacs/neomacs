use std::sync::Arc;

use parking_lot::{RwLock, RwLockReadGuard};

use crate::buffer::BufferState;

#[derive(Clone, Debug, Default)]
pub struct State {
    pub buffers: BufferState,
}

#[derive(Clone, Debug)]
pub struct StateManager<S: Clone> {
    state: Arc<RwLock<S>>,
}

impl<S: Clone> StateManager<S> {
    pub fn new(managed: S) -> Self {
        Self {
            state: Arc::new(RwLock::new(managed)),
        }
    }

    pub fn read(&self) -> RwLockReadGuard<S> {
        self.state.read()
    }

    pub fn snapshot(&self) -> S {
        self.state.read().clone()
    }

    pub fn mutate(&mut self, mutator: fn(&mut S)) {
        let mut state = self.state.write();
        mutator(&mut state)
    }
}

#[cfg(test)]
mod tests {
    use crate::state::StateManager;

    #[test]
    fn test_basics() {
        #[derive(Clone, Default, Debug, PartialEq)]
        struct MyState {
            foo: String,
            bar: u32,
            baz: Vec<f32>,
        }
        let mut state = StateManager::new(MyState::default());
        assert_eq!(
            MyState {
                foo: "".to_string(),
                bar: 0u32,
                baz: vec![]
            },
            state.snapshot()
        );
        state.mutate(|s| s.foo.push_str("foobar"));
        assert_eq!(
            MyState {
                foo: "foobar".to_string(),
                bar: 0u32,
                baz: vec![]
            },
            state.snapshot()
        );
        assert_eq!(*state.read(), state.snapshot());
    }
}
